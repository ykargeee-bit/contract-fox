#![no_std]

use soroban_sdk::{Address, Env, IntoVal, Map, Symbol, Vec, contract, contractimpl, contracttype, symbol_short, vec};

// Storage keys
const DONATION_MAP: Symbol = symbol_short!("DON_MAP");
const CAMPAIGN_TOTALS: Symbol = symbol_short!("CMP_TOT");
const DONOR_HISTORY: Symbol = symbol_short!("DON_HIS");
const DONATION_COUNT: Symbol = symbol_short!("DON_CNT");
const CAMPAIGN_CONTRACT_ID: Symbol = symbol_short!("CMP_CID");
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
    /// Initialize the donation contract with Campaign contract ID and admin address
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `campaign_contract_id` - The contract ID of the Campaign contract
    /// * `admin` - The admin allowed to pause/unpause the contract
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
            vec![
                &env,
                campaign_id.into_val(&env),
                amount.into_val(&env)
            ],
        );
    }

    /// Hook function for executing withdrawal operations request triggers
    pub fn request_withdrawal(env: Env, campaign_id: u64, withdrawal_id: u64, amount: i128) {
        require_not_paused(&env);

        if amount <= 0 {
            panic!("Withdrawal request amount must be positive");
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
    pub fn approve_withdrawal(env: Env, withdrawal_id: u64, tx_hash: Symbol) {
        require_not_paused(&env);

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
    use soroban_sdk::{Address, Env, Map, Symbol, contract, contractimpl, testutils::Address as _};
    use crate::{DonationContract, DonationContractClient};
    
    const MOCK_CAMP_MAP: Symbol = soroban_sdk::symbol_short!("CMP_MAP");

    // Mock Campaign contract for testing
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
        
        pub fn get_raised_amount(env: Env, campaign_id: u64) -> i128 {
            let store: Map<u64, i128> = env.storage().instance().get(&MOCK_CAMP_MAP).unwrap_or(Map::new(&env));
            store.get(campaign_id).unwrap_or(0)
        }
    }

    #[test]
    fn test_donate_and_get_total_raised() {
        let env = Env::default();
        env.mock_all_auths();
        
        // First, deploy a mock Campaign contract
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        
        // Deploy Donation contract
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        // Initialize with Campaign contract ID
        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);

        let donor = Address::generate(&env);
        let campaign_id = 1u64;
        let amount = 100i128;

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
        
        // First, deploy a mock Campaign contract
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        
        // Deploy Donation contract
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        // Initialize with Campaign contract ID
        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);

        let donor1 = Address::generate(&env);
        let donor2 = Address::generate(&env);
        let campaign_id = 1u64;

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
        
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let donor = Address::generate(&env);
        let campaign_id = 1u64;

        client.donate(&donor, &campaign_id, &0i128);
    }

    #[test]
    #[should_panic(expected = "Amount must be positive")]
    fn test_donate_negative_amount() {
        let env = Env::default();
        env.mock_all_auths();
        
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        let donor = Address::generate(&env);
        let campaign_id = 1u64;

        client.donate(&donor, &campaign_id, &-100i128);
    }
    
    #[test]
    #[should_panic(expected = "Campaign contract ID not set")]
    fn test_donate_without_initialization() {
        let env = Env::default();
        
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let donor = Address::generate(&env);
        let campaign_id = 1u64;
        let amount = 100i128;
        
        client.donate(&donor, &campaign_id, &amount);
    }

    #[test]
    #[should_panic(expected = "Donation contract instance is already initialized")]
    fn test_prevent_double_initialization() {
        let env = Env::default();
        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        client.initialize(&mock_campaign_id, &admin);
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn test_donate_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);
        client.pause(&admin);

        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &100i128);
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let mock_campaign_id = env.register_contract(None, MockCampaignContract);
        let contract_id = env.register_contract(None, DonationContract);
        let client = DonationContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.initialize(&mock_campaign_id, &admin);

        client.pause(&admin);
        client.unpause(&admin);

        let donor = Address::generate(&env);
        client.donate(&donor, &1u64, &50i128);
        assert_eq!(client.get_total_raised(&1u64), 50i128);
    }
}
