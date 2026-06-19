#![no_std]

use soroban_sdk::{Address, Env, Symbol, Vec, contract, contractimpl, contracttype, symbol_short};

const CAMPAIGN_TTL_THRESHOLD_LEDGERS: u32 = 17280 * 7;
const CAMPAIGN_TTL_BUMP_TO_LEDGERS: u32 = 17280 * 30;
const CAMPAIGN_TTL_BUMP_LOCK_WINDOW_LEDGERS: u32 = 100;
const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");

// Campaign status constants
pub const CAMPAIGN_STATUS_ACTIVE: u32 = 0;
pub const CAMPAIGN_STATUS_COMPLETED: u32 = 1;
pub const CAMPAIGN_STATUS_CANCELLED: u32 = 2;
pub const CAMPAIGN_STATUS_EXPIRED: u32 = 3;

// Campaign data tuple: (id, owner, goal, deadline, status, created_at)
pub type Campaign = (u64, Address, i128, u64, u32, u64);

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum DataKey {
    CampaignCount,
    Campaign(u64),
    Raised(u64),
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

    let raised_key = DataKey::Raised(campaign_id);
    env.storage().persistent().extend_ttl(
        &raised_key,
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

// --- Structured Events for Off-Chain Indexing ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignRegisteredEvent {
    pub campaign_id: u64,
    pub owner: Address,
    pub goal: i128,
    pub deadline: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignStatusChangedEvent {
    pub campaign_id: u64,
    pub old_status: u32,
    pub new_status: u32,
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
    /// Initialize the contract and set the admin address
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN) {
            panic!("Contract is already initialized");
        }
        env.storage().instance().set(&ADMIN, &admin);
    }

    /// Pause the contract; only the admin can call this
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

    /// Unpause the contract; only the admin can call this
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
    ///
    /// # Returns
    /// The ID of newly created campaign
    pub fn register_campaign(env: Env, owner: Address, goal: i128, deadline: u64) -> u64 {
        require_not_paused(&env);
        owner.require_auth();

        // Get current campaign count and increment
        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::CampaignCount)
            .unwrap_or(0);
        count += 1;

        // Create new campaign tuple: (id, owner, goal, deadline, status, created_at)
        let campaign: Campaign = (
            count,
            owner.clone(),
            goal,
            deadline,
            CAMPAIGN_STATUS_ACTIVE,
            env.ledger().timestamp(),
        );

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(count), &campaign);
        env.storage().persistent().set(&DataKey::Raised(count), &0i128);
        env.storage().persistent().set(&DataKey::CampaignCount, &count);
        bump_campaign_index_ttl(&env);
        bump_campaign_ttl(&env, count);

        // Emit Structured Event for Indexers
        env.events().publish(
            (Symbol::new(&env, "CampaignRegistered"), count),
            CampaignRegisteredEvent {
                campaign_id: count,
                owner,
                goal,
                deadline,
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
    /// The Campaign tuple if found
    pub fn get_campaign(env: Env, campaign_id: u64) -> Campaign {
        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        let (_, _, _, _, status, _) = &campaign;
        if *status == CAMPAIGN_STATUS_ACTIVE {
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
    pub fn update_campaign_status(env: Env, campaign_id: u64, status: u32) {
        require_not_paused(&env);
        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        // Extract campaign data
        let (id, owner, goal, deadline, old_status, created_at) = campaign;

        // Only campaign owner can update status
        owner.require_auth();

        // Create updated campaign tuple
        let updated_campaign: Campaign = (id, owner, goal, deadline, status, created_at);

        env.storage()
            .persistent()
            .set(&DataKey::Campaign(campaign_id), &updated_campaign);

        if status == CAMPAIGN_STATUS_ACTIVE {
            bump_campaign_ttl(&env, campaign_id);
        }

        // Emit Structured Event for Indexers
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
                let (_, _, _, _, status, _) = &campaign;
                if *status == CAMPAIGN_STATUS_ACTIVE {
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

        // Validate campaign exists before updating balances
        let campaign: Campaign = env
            .storage()
            .persistent()
            .get(&DataKey::Campaign(campaign_id))
            .unwrap_or_else(|| panic!("Campaign not found"));

        let (_, _, _, _, status, _) = &campaign;
        if *status == CAMPAIGN_STATUS_ACTIVE {
            bump_campaign_ttl(&env, campaign_id);
        }

        let current_raised: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Raised(campaign_id))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::Raised(campaign_id), &(current_raised + amount));
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
            let (_, _, _, _, status, _) = &campaign;
            if *status == CAMPAIGN_STATUS_ACTIVE {
                bump_campaign_ttl(&env, campaign_id);
            }
        }

        env.storage()
            .persistent()
            .get(&DataKey::Raised(campaign_id))
            .unwrap_or(0)
    }
}
