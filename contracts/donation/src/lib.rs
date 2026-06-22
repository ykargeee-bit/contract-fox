#![no_std]

use soroban_sdk::{Address, Bytes, Env, IntoVal, Map, Symbol, Vec, contract, contractimpl, contracttype, symbol_short, token, vec as soroban_vec};
use contracts_shared::Campaign;

// Storage keys
const DONATION_MAP: Symbol = symbol_short!("DON_MAP");
const CAMPAIGN_TOTALS: Symbol = symbol_short!("CMP_TOT");
const DONOR_HISTORY: Symbol = symbol_short!("DON_HIS");
const DONATION_COUNT: Symbol = symbol_short!("DON_CNT");
const CAMPAIGN_CONTRACT_ID: Symbol = symbol_short!("CMP_CID");
const ADMIN: Symbol = symbol_short!("ADMIN");
const PAUSED: Symbol = symbol_short!("PAUSED");

// Donation data tuple: (donor, campaign_id, amount, timestamp, memo)
// memo is an optional byte string capped at 28 bytes (Stellar protocol limit).
pub type Donation = (Address, u64, i128, u64, Option<Bytes>);

// --- Structured Events for Off-Chain Indexing ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationMadeEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
    pub token_id: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalRequestedEvent {
    pub campaign_id: u64,
    pub withdrawal_id: u64,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalApprovedEvent {
    pub withdrawal_id: u64,
    pub tx_hash: Symbol,
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
pub struct DonationContract;

#[contractimpl]
impl DonationContract {
    /// Initialize the donation contract with Campaign contract ID and admin address
    pub fn initialize(env: Env, campaign_contract_id: Address, admin: Address) {
        if env.storage().instance().has(&CAMPAIGN_CONTRACT_ID) {
            panic!("Donation contract instance is already initialized");
        }
        env.storage().instance().set(&CAMPAIGN_CONTRACT_ID, &campaign_contract_id);
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

    /// Donate funds to a campaign
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `donor` - The address of the donor
    /// * `campaign_id` - The ID of the campaign to donate to
    /// * `token_id` - The token contract address to donate (XLM for native)
    /// * `amount` - The amount to donate
    /// * `memo` - Optional memo text (max 28 bytes per Stellar protocol)
    pub fn donate(env: Env, donor: Address, campaign_id: u64, token_id: Address, amount: i128, memo: Option<Bytes>) {
        require_not_paused(&env);

        donor.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        // Validate memo length (Stellar protocol: max 28 bytes)
        if let Some(ref m) = memo {
            if m.len() > 28 {
                panic!("Memo must not exceed 28 bytes");
            }
        }

        let campaign_contract_id: Address = env
            .storage()
            .instance()
            .get(&CAMPAIGN_CONTRACT_ID)
            .unwrap_or_else(|| panic!("Campaign contract ID not set. Call initialize() first."));

        let campaign: Campaign = env.invoke_contract(
            &campaign_contract_id,
            &Symbol::new(&env, "get_campaign"),
            soroban_vec![&env, campaign_id.into_val(&env)],
        );

        if campaign.status != contracts_shared::CampaignStatus::Active {
            panic!("Campaign is not active");
        }

        if let Some(accepted_token) = campaign.asset_contract_id {
            if token_id != accepted_token {
                panic!("Token does not match campaign's accepted asset");
            }
        }

        token::Client::new(&env, &token_id).transfer(
            &donor,
            &env.current_contract_address(),
            &amount,
        );

        let donation: Donation = (
            donor.clone(),
            campaign_id,
            amount,
            env.ledger().timestamp(),
            memo,
        );

        let mut donation_count: u64 = env.storage().instance().get(&DONATION_COUNT).unwrap_or(0);
        donation_count += 1;
        let donation_id = donation_count;

        let mut donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));
        donations.set(donation_id, donation);
        env.storage().instance().set(&DONATION_MAP, &donations);

        env.storage().instance().set(&DONATION_COUNT, &donation_count);

        let mut campaign_totals: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_TOTALS)
            .unwrap_or(Map::new(&env));
        let current_total: i128 = campaign_totals.get(campaign_id).unwrap_or(0);
        campaign_totals.set(campaign_id, current_total + amount);
        env.storage().instance().set(&CAMPAIGN_TOTALS, &campaign_totals);

        let mut donor_history: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&DONOR_HISTORY)
            .unwrap_or(Map::new(&env));
        let mut donor_donations: Vec<u64> = donor_history.get(donor.clone()).unwrap_or(Vec::new(&env));
        donor_donations.push_back(donation_id);
        donor_history.set(donor.clone(), donor_donations);
        env.storage().instance().set(&DONOR_HISTORY, &donor_history);

        env.events().publish(
            (Symbol::new(&env, "DonationMade"), campaign_id),
            DonationMadeEvent {
                campaign_id,
                donor,
                amount,
                token_id,
            },
        );

        env.invoke_contract::<()>(
            &campaign_contract_id,
            &Symbol::new(&env, "update_raised_amount"),
            soroban_vec![&env, campaign_id.into_val(&env), amount.into_val(&env)],
        );
    }

    /// Only the campaign owner or admin may call this
    pub fn request_withdrawal(env: Env, caller: Address, campaign_id: u64, withdrawal_id: u64, amount: i128) {
        require_not_paused(&env);
        caller.require_auth();

        if amount <= 0 {
            panic!("Withdrawal request amount must be positive");
        }

        let campaign_contract_id: Address = env
            .storage()
            .instance()
            .get(&CAMPAIGN_CONTRACT_ID)
            .unwrap_or_else(|| panic!("Campaign contract ID not set. Call initialize() first."));

        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .unwrap_or_else(|| panic!("Contract not initialized"));

        let campaign: Campaign = env.invoke_contract(
            &campaign_contract_id,
            &Symbol::new(&env, "get_campaign"),
            soroban_vec![&env, campaign_id.into_val(&env)],
        );

        if caller != campaign.owner && caller != admin {
            panic!("Unauthorized: caller is not campaign owner or admin");
        }

        env.events().publish(
            (symbol_short!("with_req"), campaign_id),
            WithdrawalRequestedEvent { campaign_id, withdrawal_id, amount },
        );
    }

    /// Only the admin may call this
    pub fn approve_withdrawal(env: Env, caller: Address, withdrawal_id: u64, tx_hash: Symbol) {
        require_not_paused(&env);
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .unwrap_or_else(|| panic!("Contract not initialized"));

        if caller != admin {
            panic!("Unauthorized: caller is not admin");
        }

        env.events().publish(
            (symbol_short!("with_app"), withdrawal_id),
            WithdrawalApprovedEvent { withdrawal_id, tx_hash },
        );
    }

    pub fn get_donations_for_campaign(env: Env, campaign_id: u64) -> Vec<Donation> {
        let donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));

        let mut result = Vec::new(&env);
        for key in donations.keys() {
            if let Some(donation) = donations.get(key) {
                let (_, cid, _, _, _) = donation.clone();
                if cid == campaign_id {
                    result.push_back(donation);
                }
            }
        }
        result
    }

    pub fn get_total_raised(env: Env, campaign_id: u64) -> i128 {
        let campaign_totals: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_TOTALS)
            .unwrap_or(Map::new(&env));
        campaign_totals.get(campaign_id).unwrap_or(0)
    }

    pub fn get_donor_history(env: Env, donor: Address) -> Vec<Donation> {
        let donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));

        let donor_history: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&DONOR_HISTORY)
            .unwrap_or(Map::new(&env));

        let mut result = Vec::new(&env);
        if let Some(keys) = donor_history.get(donor) {
            for key in keys.iter() {
                if let Some(donation) = donations.get(key) {
                    result.push_back(donation);
                }
            }
        }
        result
    }

    pub fn get_all_donations(env: Env) -> Vec<Donation> {
        let donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));

        let mut result = Vec::new(&env);
        for key in donations.keys() {
            if let Some(donation) = donations.get(key) {
                result.push_back(donation);
            }
        }
        result
    }
}

#[cfg(test)]
mod test {
    use soroban_sdk::{Address, Bytes, Env, Map, Symbol, contract, contractimpl, testutils::Address as _, token::StellarAssetClient};
    use crate::{DonationContract, DonationContractClient};
    use contracts_shared::{Campaign, CampaignStatus};

    #[contract]
    pub struct MockCampaignContract;

    #[contractimpl]
    impl MockCampaignContract {
        pub fn update_raised_amount(env: Env, _campaign_id: u64, amount: i128) {
            if amount <= 0 { panic!("Amount must be positive"); }
            let mut store: Map<u64, i128> = env.storage().instance().get(&Symbol::new(&env, "RAISED")).unwrap_or(Map::new(&env));
            store.set(1u64, store.get(1u64).unwrap_or(0) + amount);
            env.storage().instance().set(&Symbol::new(&env, "RAISED"), &store);
        }

        pub fn set_campaign_owner(env: Env, campaign_id: u64, owner: Address) {
            let mut store: Map<u64, Address> = env.storage().instance().get(&Symbol::new(&env, "OWNER")).unwrap_or(Map::new(&env));
            store.set(campaign_id, owner);
            env.storage().instance().set(&Symbol::new(&env, "OWNER"), &store);
        }

        pub fn set_campaign_status(env: Env, campaign_id: u64, status: u32) {
            let mut store: Map<u64, u32> = env.storage().instance().get(&Symbol::new(&env, "STATUS")).unwrap_or(Map::new(&env));
            store.set(campaign_id, status);
            env.storage().instance().set(&Symbol::new(&env, "STATUS"), &store);
        }

        pub fn set_campaign_asset(env: Env, campaign_id: u64, asset: Address) {
            let mut store: Map<u64, Address> = env.storage().instance().get(&Symbol::new(&env, "ASSET")).unwrap_or(Map::new(&env));
            store.set(campaign_id, asset);
            env.storage().instance().set(&Symbol::new(&env, "ASSET"), &store);
        }

        pub fn get_campaign(env: Env, campaign_id: u64) -> Campaign {
            let owners: Map<u64, Address> = env.storage().instance().get(&Symbol::new(&env, "OWNER")).unwrap_or(Map::new(&env));
            let statuses: Map<u64, u32> = env.storage().instance().get(&Symbol::new(&env, "STATUS")).unwrap_or(Map::new(&env));
            let assets: Map<u64, Address> = env.storage().instance().get(&Symbol::new(&env, "ASSET")).unwrap_or(Map::new(&env));
            let owner = owners.get(campaign_id).unwrap_or_else(|| panic!("Campaign not found"));
            let status = match statuses.get(campaign_id).unwrap_or(0) {
                0 => CampaignStatus::Active,
                1 => CampaignStatus::Completed,
                2 => CampaignStatus::Suspended,
                _ => CampaignStatus::Rejected,
            };
            Campaign {
                id: campaign_id,
                owner,
                goal: 1000i128,
                raised: 0,
                status,
                deadline: 9999999u64,
                asset_contract_id: assets.get(campaign_id),
            }
        }
    }

    fn setup(env: &Env) -> (DonationContractClient, Address, Address) {
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(env, &contract_id);
        let token_id = env.register_stellar_asset_contract(Address::generate(env));
        let admin = Address::generate(env);
        client.initialize(&mock_campaign_id, &admin);
        (client, token_id, admin)
    }

    #[test]
    fn test_donate_and_get_total_raised() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);
        let amount = 100i128;
        StellarAssetClient::new(&env, &token_id).mint(&donor, &amount);

        client.donate(&donor, &campaign_id, &token_id, &amount, &None);

        assert_eq!(client.get_total_raised(&campaign_id), amount);

        let donations = client.get_donations_for_campaign(&campaign_id);
        assert_eq!(donations.len(), 1);
        let (donor_addr, cid, donated_amount, _, memo) = donations.get(0).unwrap();
        assert_eq!(donor_addr, donor);
        assert_eq!(cid, campaign_id);
        assert_eq!(donated_amount, amount);
        assert_eq!(memo, None);

        let history = client.get_donor_history(&donor);
        assert_eq!(history.len(), 1);
        let (donor_addr2, cid2, amount2, _, _) = history.get(0).unwrap();
        assert_eq!(donor_addr2, donor);
        assert_eq!(cid2, campaign_id);
        assert_eq!(amount2, amount);
    }

    #[test]
    fn test_multiple_donations() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&donor1, &100i128);
        StellarAssetClient::new(&env, &token_id).mint(&donor2, &200i128);

        client.donate(&donor1, &campaign_id, &token_id, &100i128, &None);
        client.donate(&donor2, &campaign_id, &token_id, &200i128, &None);

        assert_eq!(client.get_total_raised(&campaign_id), 300i128);
        assert_eq!(client.get_donations_for_campaign(&campaign_id).len(), 2);
        assert_eq!(client.get_donor_history(&donor1).len(), 1);
        assert_eq!(client.get_donor_history(&donor2).len(), 1);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        mock_client.set_campaign_owner(&1u64, &campaign_owner);

        client.pause(&admin);
        client.unpause(&admin);

        let donor = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&donor, &50i128);
        client.donate(&donor, &1u64, &token_id, &50i128, &None);
        assert_eq!(client.get_total_raised(&1u64), 50i128);
    }

    #[test]
    fn test_request_and_approve_withdrawal() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        mock_client.set_campaign_owner(&1u64, &campaign_owner);

        client.request_withdrawal(&campaign_owner, &1u64, &1u64, &100i128);
        client.request_withdrawal(&admin, &1u64, &2u64, &200i128);
        client.approve_withdrawal(&admin, &1u64, &Symbol::new(&env, "txhash123"));
    }

    #[test]
    fn test_donate_with_custom_token() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let usdc_token = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);
        mock_client.set_campaign_asset(&campaign_id, &usdc_token);

        let donor = Address::generate(&env);
        StellarAssetClient::new(&env, &usdc_token).mint(&donor, &100i128);
        client.donate(&donor, &campaign_id, &usdc_token, &100i128, &None);

        assert_eq!(client.get_total_raised(&campaign_id), 100i128);
    }

    #[test]
    fn test_donate_with_memo() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&donor, &100i128);

        let memo = Bytes::from_slice(&env, b"for my friend");
        client.donate(&donor, &campaign_id, &token_id, &100i128, &Some(memo.clone()));

        let (_, _, _, _, stored_memo) = client.get_donations_for_campaign(&campaign_id).get(0).unwrap();
        assert_eq!(stored_memo, Some(memo));
    }

    #[test]
    fn test_donate_memo_max_length() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract(Address::generate(&env));
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);
        StellarAssetClient::new(&env, &token_id).mint(&donor, &100i128);

        // Exactly 28 bytes — should succeed
        let memo = Bytes::from_slice(&env, b"exactly28byteslong!!!!!!!!!!"); // 28 bytes
        client.donate(&donor, &campaign_id, &token_id, &100i128, &Some(memo.clone()));

        let (_, _, _, _, stored_memo) = client.get_donations_for_campaign(&campaign_id).get(0).unwrap();
        assert_eq!(stored_memo, Some(memo));
    }
}
