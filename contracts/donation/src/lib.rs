#![no_std]

use soroban_sdk::{Address, Env, IntoVal, Map, Symbol, Vec, contract, contractimpl, contracttype, symbol_short, token, vec as soroban_vec};

// Storage keys
const DONATION_MAP: Symbol = symbol_short!("DON_MAP");
const CAMPAIGN_TOTALS: Symbol = symbol_short!("CMP_TOT");
const DONOR_HISTORY: Symbol = symbol_short!("DON_HIS");
const DONATION_COUNT: Symbol = symbol_short!("DON_CNT");
const CAMPAIGN_CONTRACT_ID: Symbol = symbol_short!("CMP_CID");
const TOKEN_ID: Symbol = symbol_short!("TOKEN_ID");
const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");

// Donation data tuple: (donor, campaign_id, amount, timestamp)
pub type Donation = (Address, u64, i128, u64);

// --- Structured Events for Off-Chain Indexing ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonationMadeEvent {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
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
    /// Initialize the donation contract with Campaign contract ID, token ID, and admin address
    pub fn initialize(env: Env, campaign_contract_id: Address, token_id: Address, admin: Address) {
        if env.storage().instance().has(&CAMPAIGN_CONTRACT_ID) {
            panic!("Donation contract instance is already initialized");
        }
        env.storage().instance().set(&CAMPAIGN_CONTRACT_ID, &campaign_contract_id);
        env.storage().instance().set(&TOKEN_ID, &token_id);
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
    /// * `amount` - The amount to donate
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128) {
        require_not_paused(&env);

        // Require authentication from donor
        donor.require_auth();

        // Validate amount is positive
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        // Fetch Campaign contract ID for cross-contract execution lookup verification phase
        let campaign_contract_id: Address = env
            .storage()
            .instance()
            .get(&CAMPAIGN_CONTRACT_ID)
            .unwrap_or_else(|| panic!("Campaign contract ID not set. Call initialize() first."));

        let token_id: Address = env
            .storage()
            .instance()
            .get(&TOKEN_ID)
            .unwrap_or_else(|| panic!("Token ID not set. Call initialize() first."));

        // Transfer XLM from donor to this contract
        token::Client::new(&env, &token_id).transfer(
            &donor,
            &env.current_contract_address(),
            &amount,
        );

        // Validate the campaign exists and is active (status == 0)
        let campaign: (u64, Address, i128, u64, u32, u64) = env.invoke_contract(
            &campaign_contract_id,
            &symbol_short!("get_campaign"),
            (campaign_id,),
        );
        let (_, _, _, _, status, _) = campaign;
        if status != 0 {
            panic!("Campaign is not active");
        }

        // Create donation record
        let donation: Donation = (
            donor.clone(),
            campaign_id,
            amount,
            env.ledger().timestamp(),
        );

        // Get next donation ID
        let mut donation_count: u64 = env.storage().instance().get(&DONATION_COUNT).unwrap_or(0);
        donation_count += 1;
        let donation_id = donation_count;

        // Store donation in donations map
        let mut donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));
        donations.set(donation_id, donation);
        env.storage().instance().set(&DONATION_MAP, &donations);

        // Update donation count
        env.storage().instance().set(&DONATION_COUNT, &donation_count);

        // Update campaign totals
        let mut campaign_totals: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_TOTALS)
            .unwrap_or(Map::new(&env));
        let current_total: i128 = campaign_totals.get(campaign_id).unwrap_or(0);
        campaign_totals.set(campaign_id, current_total + amount);
        env.storage().instance().set(&CAMPAIGN_TOTALS, &campaign_totals);

        // Update donor history
        let mut donor_history: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&DONOR_HISTORY)
            .unwrap_or(Map::new(&env));
        let mut donor_donations: Vec<u64> = donor_history.get(donor.clone()).unwrap_or(Vec::new(&env));
        donor_donations.push_back(donation_id);
        donor_history.set(donor.clone(), donor_donations);
        env.storage().instance().set(&DONOR_HISTORY, &donor_history);

        // Emit Structured Event for Indexers
        env.events().publish(
            (Symbol::new(&env, "DonationMade"), campaign_id),
            DonationMadeEvent {
                campaign_id,
                donor,
                amount,
            },
        );

        // Cross-call the Campaign contract to update raised amount natively
        env.invoke_contract::<()>(
            &campaign_contract_id,
            &Symbol::new(&env, "update_raised_amount"),
            soroban_vec![&env, campaign_id.into_val(&env), amount.into_val(&env)],
            vec![
                &env,
                campaign_id.into_val(&env),
                amount.into_val(&env)
            ],
        );
    }

    /// Hook function for executing withdrawal operations request triggers
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

        // Fetch campaign to verify caller is campaign owner or admin
        let campaign: (u64, Address, i128, u64, u32, u64) = env.invoke_contract(
            &campaign_contract_id,
            &symbol_short!("get_campaign"),
            (campaign_id,),
        );
        let (_, campaign_owner, _, _, _, _) = campaign;

        if caller != campaign_owner && caller != admin {
            panic!("Unauthorized: caller is not campaign owner or admin");
        }

        
        env.events().publish(
            (symbol_short!("with_req"), campaign_id),
            WithdrawalRequestedEvent {
                campaign_id,
                withdrawal_id,
                amount,
            },
        );
    }

    /// Hook function for validating completed withdrawal distributions
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
            WithdrawalApprovedEvent {
                withdrawal_id,
                tx_hash,
            },
        );
    }

    /// Get all donations for a specific campaign
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of the campaign
    ///
    /// # Returns
    /// Vector of Donation tuples for the campaign
    pub fn get_donations_for_campaign(env: Env, campaign_id: u64) -> Vec<Donation> {
        let donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));

        let mut result = Vec::new(&env);
        let keys = donations.keys();

        for key in keys {
            if let Some(donation) = donations.get(key) {
                let (_, donation_campaign_id, _, _) = donation;
                if donation_campaign_id == campaign_id {
                    result.push_back(donation);
                }
            }
        }

        result
    }

    /// Get total raised amount for a specific campaign
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_id` - The ID of the campaign
    ///
    /// # Returns
    /// Total amount raised for the campaign
    pub fn get_total_raised(env: Env, campaign_id: u64) -> i128 {
        let campaign_totals: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_TOTALS)
            .unwrap_or(Map::new(&env));

        campaign_totals.get(campaign_id).unwrap_or(0)
    }

    /// Get donation history for a specific donor
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `donor` - The address of the donor
    ///
    /// # Returns
    /// Vector of Donation tuples made by the donor
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

        if let Some(donation_keys) = donor_history.get(donor) {
            for donation_key in donation_keys.iter() {
                if let Some(donation) = donations.get(donation_key) {
                    result.push_back(donation);
                }
            }
        }

        result
    }

    /// Get all donations (utility function for testing)
    ///
    /// # Arguments
    /// * `env` - The contract environment
    ///
    /// # Returns
    /// Vector of all donations
    pub fn get_all_donations(env: Env) -> Vec<Donation> {
        let donations: Map<u64, Donation> = env
            .storage()
            .instance()
            .get(&DONATION_MAP)
            .unwrap_or(Map::new(&env));

        let mut result = Vec::new(&env);
        let keys = donations.keys();

        for key in keys {
            if let Some(donation) = donations.get(key) {
                result.push_back(donation);
            }
        }

        result
    }
}

#[cfg(test)]
mod test {
    use soroban_sdk::{Address, Env, Map, Symbol, contract, contractimpl, testutils::Address as _, token::Client as TokenClient};
    use crate::{DonationContract, DonationContractClient};

    const MOCK_CAMP_MAP: Symbol = symbol_short!("CMP_MAP");
    const MOCK_CAMP_STATUS: Symbol = symbol_short!("CMP_STA");
    const MOCK_CAMP_OWNER: Symbol = symbol_short!("CMP_OWN");
    
    const MOCK_CAMP_MAP: Symbol = soroban_sdk::symbol_short!("CMP_MAP");

    // Mock Campaign contract for testing
    // Campaign tuple: (id, owner, goal, deadline, status, created_at)
    #[contract]
    pub struct MockCampaignContract;
    
    #[contractimpl]
    impl MockCampaignContract {
        pub fn update_raised_amount(env: Env, campaign_id: u64, amount: i128) {
            if amount <= 0 {
                panic!("Amount must be positive");
            }
            let mut store: Map<u64, i128> = env.storage().instance().get(&MOCK_CAMP_MAP).unwrap_or(Map::new(&env));
            let current = store.get(campaign_id).unwrap_or(0);
            store.set(campaign_id, current + amount);
            env.storage().instance().set(&MOCK_CAMP_MAP, &store);
        }

        /// Set the owner for a campaign (for test setup)
        pub fn set_campaign_owner(env: Env, campaign_id: u64, owner: Address) {
            let mut store: Map<u64, Address> = env.storage().instance().get(&MOCK_CAMP_OWNER).unwrap_or(Map::new(&env));
            store.set(campaign_id, owner);
            env.storage().instance().set(&MOCK_CAMP_OWNER, &store);
        }

        /// Set the status for a campaign (for test setup)
        pub fn set_campaign_status(env: Env, campaign_id: u64, status: u32) {
            let mut store: Map<u64, u32> = env.storage().instance().get(&MOCK_CAMP_STATUS).unwrap_or(Map::new(&env));
            store.set(campaign_id, status);
            env.storage().instance().set(&MOCK_CAMP_STATUS, &store);
        }

        /// Returns (id, owner, goal, deadline, status, created_at)
        pub fn get_campaign(env: Env, campaign_id: u64) -> (u64, Address, i128, u64, u32, u64) {
            let owners: Map<u64, Address> = env.storage().instance().get(&MOCK_CAMP_OWNER).unwrap_or(Map::new(&env));
            let statuses: Map<u64, u32> = env.storage().instance().get(&MOCK_CAMP_STATUS).unwrap_or(Map::new(&env));
            let owner = owners.get(campaign_id).unwrap_or_else(|| panic!("Campaign not found"));
            let status = statuses.get(campaign_id).unwrap_or(0); // default: active
            (campaign_id, owner, 1000i128, 9999999u64, status, 0u64)
        
        pub fn get_raised_amount(env: Env, campaign_id: u64) -> i128 {
            let store: Map<u64, i128> = env.storage().instance().get(&MOCK_CAMP_MAP).unwrap_or(Map::new(&env));
            store.get(campaign_id).unwrap_or(0)
        }
    }

    fn setup(env: &Env) -> (DonationContractClient, Address, Address) {
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(env, &contract_id);

        // Register native token and mint to donor
        let token_id = env.register_stellar_asset_contract_v2(Address::generate(env)).address();

        let admin = Address::generate(env);
        client.initialize(&mock_campaign_id, &token_id, &admin);

        (client, token_id, admin)
    }

    #[test]
    fn test_donate_and_get_total_raised() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _admin) = setup(&env);
        
        // First, deploy a mock Campaign contract
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        
        // Deploy Donation contract
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        // Initialize with Campaign contract ID
        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);
        let amount = 100i128;

        // Mint tokens to donor so transfer can succeed
        TokenClient::new(&env, &token_id).mint(&donor, &amount);

        // Test donation
        client.donate(&donor, &campaign_id, &amount);

        // Test get_total_raised
        let total_raised = client.get_total_raised(&campaign_id);
        assert_eq!(total_raised, amount);

        // Test get_donations_for_campaign
        let donations = client.get_donations_for_campaign(&campaign_id);
        assert_eq!(donations.len(), 1);
        let donation = donations.get(0).unwrap();
        let (donor_addr, donation_campaign_id, donation_amount, _) = donation;
        assert_eq!(donor_addr, donor);
        assert_eq!(donation_campaign_id, campaign_id);
        assert_eq!(donation_amount, amount);

        // Test get_donor_history
        let donor_history = client.get_donor_history(&donor);
        assert_eq!(donor_history.len(), 1);
        let donor_donation = donor_history.get(0).unwrap();
        let (donor_addr2, donation_campaign_id2, donation_amount2, _) = donor_donation;
        assert_eq!(donor_addr2, donor);
        assert_eq!(donation_campaign_id2, campaign_id);
        assert_eq!(donation_amount2, amount);
    }

    #[test]
    fn test_multiple_donations() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _admin) = setup(&env);
        
        // First, deploy a mock Campaign contract
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        
        // Deploy Donation contract
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        // Initialize with Campaign contract ID
        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);

        TokenClient::new(&env, &token_id).mint(&donor1, &100i128);
        TokenClient::new(&env, &token_id).mint(&donor2, &200i128);

        // First donation
        client.donate(&donor1, &campaign_id, &100i128);
        
        // Second donation
        client.donate(&donor2, &campaign_id, &200i128);

        // Check total raised
        let total_raised = client.get_total_raised(&campaign_id);
        assert_eq!(total_raised, 300i128);

        // Check donations for campaign
        let donations = client.get_donations_for_campaign(&campaign_id);
        assert_eq!(donations.len(), 2);

        // Check donor1 history
        let donor1_history = client.get_donor_history(&donor1);
        assert_eq!(donor1_history.len(), 1);

        // Check donor2 history
        let donor2_history = client.get_donor_history(&donor2);
        assert_eq!(donor2_history.len(), 1);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_donate_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _, _) = setup(&env);
        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &0i128);
        
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);

        client.donate(&donor, &campaign_id, &0i128);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_donate_negative_amount() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _, _) = setup(&env);
        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &-100i128);
        
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let campaign_id = 1u64;
        mock_client.set_campaign_owner(&campaign_id, &campaign_owner);

        let donor = Address::generate(&env);

        client.donate(&donor, &campaign_id, &-100i128);
    }
    
    #[test]
    #[should_panic(expected = "Campaign contract ID not set")]
    fn test_donate_without_initialization() {
        let env = Env::default();
        
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &100i128);
        let campaign_id = 1u64;
        let amount = 100i128;
        
        client.donate(&donor, &campaign_id, &amount);
    }

    #[test]
    #[should_panic(expected = "Donation contract instance is already initialized")]
    fn test_prevent_double_initialization() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &token_id, &admin);
        client.initialize(&mock_campaign_id, &token_id, &admin);
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn test_donate_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _, admin) = setup(&env);
        client.pause(&admin);

        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &100i128);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, admin) = setup(&env);
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        mock_client.set_campaign_owner(&1u64, &campaign_owner);

        client.pause(&admin);
        client.unpause(&admin);

        let donor = Address::generate(&env);
        TokenClient::new(&env, &token_id).mint(&donor, &50i128);
        client.donate(&donor, &1u64, &50i128);
        assert_eq!(client.get_total_raised(&1u64), 50i128);
    }

    // --- Authorization failure tests ---

    #[test]
    #[should_panic(expected = "Unauthorized: caller is not campaign owner or admin")]
    fn test_request_withdrawal_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let mock_client = MockCampaignContractClient::new(&env, &mock_campaign_id);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let campaign_owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        mock_client.set_campaign_owner(&1u64, &campaign_owner);

        // stranger is neither owner nor admin
        client.request_withdrawal(&stranger, &1u64, &1u64, &100i128);
    }

    #[test]
    fn test_request_withdrawal_by_campaign_owner() {
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

        // campaign owner should succeed
        client.request_withdrawal(&campaign_owner, &1u64, &1u64, &100i128);
    }

    #[test]
    fn test_request_withdrawal_by_admin() {
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

        // admin should succeed
        client.request_withdrawal(&admin, &1u64, &1u64, &100i128);
    }

    #[test]
    #[should_panic(expected = "Unauthorized: caller is not admin")]
    fn test_approve_withdrawal_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let stranger = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);

        // stranger is not admin
        client.approve_withdrawal(&stranger, &1u64, &Symbol::new(&env, "txhash123"));
    }

    #[test]
    fn test_approve_withdrawal_by_admin() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);

        // admin should succeed
        client.approve_withdrawal(&admin, &1u64, &Symbol::new(&env, "txhash123"));
    }

    #[test]
    #[should_panic(expected = "Campaign is not active")]
    fn test_donate_to_inactive_campaign() {
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
        mock_client.set_campaign_status(&1u64, &1u32); // 1 = Completed (not active)

        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &100i128);
    }

    #[test]
    #[should_panic(expected = "Campaign not found")]
    fn test_donate_to_nonexistent_campaign() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        // No campaign set up — get_campaign will panic "Campaign not found"

        let donor = Address::generate(&env);
        client.donate(&donor, &99u64, &100i128);
    }
}
