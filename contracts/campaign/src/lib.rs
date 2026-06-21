#![no_std]

use contracts_shared::{Campaign, CampaignStatus};
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

const CAMPAIGN_TTL_THRESHOLD_LEDGERS: u32 = 17280 * 7;
const CAMPAIGN_TTL_BUMP_TO_LEDGERS: u32 = 17280 * 30;
const CAMPAIGN_TTL_BUMP_LOCK_WINDOW_LEDGERS: u32 = 100;
const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");

pub const CAMPAIGN_STATUS_ACTIVE: u32 = 0;
pub const CAMPAIGN_STATUS_COMPLETED: u32 = 1;
pub const CAMPAIGN_STATUS_CANCELLED: u32 = 2;
pub const CAMPAIGN_STATUS_EXPIRED: u32 = 3;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    CampaignCount,
    Campaign(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum TempKey {
    CampaignTtlBumpLock(u64),
}

fn bump_campaign_ttl(env: &Env, campaign_id: u64) {
    let lock_key = TempKey::CampaignTtlBumpLock(campaign_id);
    let current_ledger = env.ledger().sequence();
    if let Some(last_bumped) = env.storage().temporary().get::<_, u32>(&lock_key) {
        if current_ledger.saturating_sub(last_bumped) < CAMPAIGN_TTL_BUMP_LOCK_WINDOW_LEDGERS {
            return;
        }
    }

    let campaign_key = DataKey::Campaign(campaign_id);
    env.storage().persistent().extend_ttl(
        &campaign_key,
        CAMPAIGN_TTL_THRESHOLD_LEDGERS,
        CAMPAIGN_TTL_BUMP_TO_LEDGERS,
    );

    env.storage().temporary().set(&lock_key, &current_ledger);
}

fn bump_campaign_index_ttl(env: &Env) {
    let key = DataKey::CampaignCount;
    env.storage().persistent().extend_ttl(
        &key,
        CAMPAIGN_TTL_THRESHOLD_LEDGERS,
        CAMPAIGN_TTL_BUMP_TO_LEDGERS,
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignRegisteredEvent {
    pub campaign_id: u64,
    pub owner: Address,
    pub goal: i128,
    pub deadline: u64,
    pub asset_contract_id: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignStatusChangedEvent {
    pub campaign_id: u64,
    pub old_status: CampaignStatus,
    pub new_status: CampaignStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractPausedEvent {
    pub admin: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUnpausedEvent {
    pub admin: Address,
}

fn require_not_paused(env: &Env) {
    let paused: bool = env.storage().instance().get(&PAUSED).unwrap_or(false);
    if paused {
        panic!("Contract is paused");
    }
}

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Contract is already initialized");
        }
        env.storage().instance().set(&ADMIN, &admin);
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        if admin != stored_admin {
            panic!("Unauthorized: caller is not admin");
        }
        env.storage().instance().set(&PAUSED, &true);
        env.events().publish(
            (Symbol::new(&env, "ContractPaused"),),
            ContractPausedEvent { admin },
        );
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .unwrap_or_else(|| panic!("Contract not initialized"));
        if admin != stored_admin {
            panic!("Unauthorized: caller is not admin");
        }
        env.storage().instance().set(&PAUSED, &false);
        env.events().publish(
            (Symbol::new(&env, "ContractUnpaused"),),
            ContractUnpausedEvent { admin },
        );
    }

    /// Register a new campaign
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `owner` - The address of campaign owner
    /// * `goal` - The funding goal for campaign
    /// * `deadline` - The deadline timestamp for campaign
    /// * `asset_contract_id` - Optional Soroban token contract ID (None for native XLM)
    ///
    /// # Returns
    /// The ID of newly created campaign
    pub fn register_campaign(env: Env, owner: Address, goal: i128, deadline: u64, asset_contract_id: Option<Address>) -> u64 {
        require_not_paused(&env);
        owner.require_auth();

        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignCount)
            .unwrap_or(0);
        count += 1;

        let campaign = Campaign {
            id: count,
            owner: owner.clone(),
            goal,
            raised: 0,
            status: CampaignStatus::Active,
            deadline,
            asset_contract_id: asset_contract_id.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(count), &campaign);
        env.storage()
            .persistent()
            .set(&DataKey::CampaignCount, &count);

        bump_campaign_index_ttl(&env);
        bump_campaign_ttl(&env, count);

        env.events().publish(
            (Symbol::new(&env, "CampaignRegistered"),),
            CampaignRegisteredEvent {
                campaign_id: count,
                owner,
                goal,
                deadline,
                asset_contract_id: asset_contract_id.clone(),
            },
        );

        count
    }

    /// Get campaign details by ID
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of campaign to retrieve
    ///
    /// # Returns
    /// The Campaign struct if found
    pub fn get_campaign(env: Env, campaign_id: u64) -> Campaign {
        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        if campaign.status == CampaignStatus::Active {
            bump_campaign_ttl(&env, campaign_id);
        }

        campaign
    }

    /// Update campaign status
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of campaign to update
    /// * `status` - The new status for campaign
    pub fn update_campaign_status(env: Env, campaign_id: u64, status: CampaignStatus) {
        require_not_paused(&env);

        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        let old_status = campaign.status.clone();

        let updated_campaign = Campaign {
            status: status.clone(),
            ..campaign.clone()
        };

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &updated_campaign);

        if status == CampaignStatus::Active {
            bump_campaign_ttl(&env, campaign_id);
        }

        env.events().publish(
            (Symbol::new(&env, "CampaignStatusUpdated"), campaign_id),
            CampaignStatusChangedEvent {
                campaign_id,
                old_status,
                new_status: status,
            },
        );
    }

    /// Get total number of campaigns
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// The total count of registered campaigns
    pub fn get_campaign_count(env: Env) -> u64 {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignCount)
            .unwrap_or(0);
        bump_campaign_index_ttl(&env);
        count
    }

    /// Get all campaigns (utility function for testing)
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// Vector of all campaigns
    pub fn get_all_campaigns(env: Env) -> Vec<Campaign> {
        let mut result = Vec::new(&env);
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignCount)
            .unwrap_or(0);
        bump_campaign_index_ttl(&env);

        for campaign_id in 1..=count {
            if let Some(campaign) = env
                .storage()
                .persistent()
                .get::<_, Campaign>(&DataKey::Campaign(campaign_id))
            {
                if campaign.status == CampaignStatus::Active {
                    bump_campaign_ttl(&env, campaign_id);
                }
                result.push_back(campaign);
            }
        }

        result
    }

    /// Update raised amount for a campaign (can be called by other contracts)
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of campaign to update
    /// * `amount` - The amount to add to raised total
    pub fn update_raised_amount(env: Env, campaign_id: u64, amount: i128) {
        require_not_paused(&env);
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        let updated_campaign = Campaign {
            raised: campaign.raised + amount,
            ..campaign
        };

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &updated_campaign);
    }

    /// Get raised amount for a campaign
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of campaign
    ///
    /// # Returns
    /// The total amount raised for the campaign
    pub fn get_raised_amount(env: Env, campaign_id: u64) -> i128 {
        if let Some(campaign) = env
            .storage()
            .persistent()
            .get::<_, Campaign>(&DataKey::Campaign(campaign_id))
        {
            if campaign.status == CampaignStatus::Active {
                bump_campaign_ttl(&env, campaign_id);
            }
            campaign.raised
        } else {
            0
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_register_and_get_campaign() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner = Address::generate(&env);
        let goal = 1000i128;
        let deadline = 1700000000u64;

        let id = client.register_campaign(&owner, &goal, &deadline, &None);
        assert_eq!(id, 1);

        let campaign = client.get_campaign(&id);
        assert_eq!(campaign.id, 1);
        assert_eq!(campaign.owner, owner);
        assert_eq!(campaign.goal, goal);
        assert_eq!(campaign.raised, 0);
        assert_eq!(campaign.status, CampaignStatus::Active);
        assert_eq!(campaign.deadline, deadline);
        assert_eq!(campaign.asset_contract_id, None);

        assert_eq!(client.get_campaign_count(), 1);
    }

    #[test]
    fn test_register_campaign_with_custom_token() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner = Address::generate(&env);
        let usdc_contract = Address::generate(&env);
        let goal = 5000i128;
        let deadline = 1800000000u64;

        let id = client.register_campaign(&owner, &goal, &deadline, &Some(usdc_contract.clone()));
        assert_eq!(id, 1);

        let campaign = client.get_campaign(&id);
        assert_eq!(campaign.asset_contract_id, Some(usdc_contract));
    }

    #[test]
    fn test_update_campaign_status() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner = Address::generate(&env);
        let id = client.register_campaign(&owner, &1000i128, &1700000000u64, &None);

        client.update_campaign_status(&id, &CampaignStatus::Completed);

        let campaign = client.get_campaign(&id);
        assert_eq!(campaign.status, CampaignStatus::Completed);
    }

    #[test]
    fn test_multiple_campaigns() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);

        let id1 = client.register_campaign(&owner1, &500i128, &1700000000u64, &None);
        let id2 = client.register_campaign(&owner2, &1000i128, &1800000000u64, &None);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(client.get_campaign_count(), 2);

        let all = client.get_all_campaigns();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_update_raised_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner = Address::generate(&env);
        let id = client.register_campaign(&owner, &1000i128, &1700000000u64, &None);

        client.update_raised_amount(&id, &250i128);
        assert_eq!(client.get_raised_amount(&id), 250);

        client.update_raised_amount(&id, &150i128);
        assert_eq!(client.get_raised_amount(&id), 400);

        let campaign = client.get_campaign(&id);
        assert_eq!(campaign.raised, 400);
    }

    #[test]
    #[should_panic(expected = "Contract is already initialized")]
    fn test_prevent_double_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
        client.initialize(&admin);
    }

    #[test]
    #[should_panic(expected = "Campaign not found")]
    fn test_get_nonexistent_campaign() {
        let env = Env::default();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        client.get_campaign(&999u64);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_update_raised_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        let owner = Address::generate(&env);
        let id = client.register_campaign(&owner, &1000i128, &1700000000u64, &None);

        client.update_raised_amount(&id, &0i128);
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn test_register_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);
        client.pause(&admin);

        let owner = Address::generate(&env);
        client.register_campaign(&owner, &1000i128, &1700000000u64, &None);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, CampaignContract);
        let client = CampaignContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&admin);

        client.pause(&admin);
        client.unpause(&admin);

        let owner = Address::generate(&env);
        let id = client.register_campaign(&owner, &1000i128, &1700000000u64, &None);
        assert_eq!(id, 1);
    }
}