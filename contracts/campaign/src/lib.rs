#![no_std]

use soroban_sdk::{Address, Env, Map, Symbol, Vec, contract, contractimpl, contracttype, symbol_short};

// Storage keys
const CAMPAIGN_MAP: Symbol = symbol_short!("CMP_MAP");
const CAMPAIGN_COUNT: Symbol = symbol_short!("CMP_CNT");
const CAMPAIGN_RAISED: Symbol = symbol_short!("CMP_RAS");
const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");

// Campaign status constants
pub const CAMPAIGN_STATUS_ACTIVE: u32 = 0;
pub const CAMPAIGN_STATUS_COMPLETED: u32 = 1;
pub const CAMPAIGN_STATUS_CANCELLED: u32 = 2;
pub const CAMPAIGN_STATUS_EXPIRED: u32 = 3;

// Campaign data tuple: (id, owner, goal, deadline, status, created_at)
pub type Campaign = (u64, Address, i128, u64, u32, u64);

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
    pub fn register_campaign(env: Env, owner: Address, goal: i128, deadline: u64) -> u64 {
        require_not_paused(&env);
        owner.require_auth();

        let mut count: u64 = env.storage().instance().get(&CAMPAIGN_COUNT).unwrap_or(0);
        count += 1;

        let campaign: Campaign = (
            count,
            owner.clone(),
            goal,
            deadline,
            CAMPAIGN_STATUS_ACTIVE,
            env.ledger().timestamp(),
        );

        let mut campaigns: Map<u64, Campaign> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_MAP)
            .unwrap_or(Map::new(&env));
        campaigns.set(count, campaign);
        env.storage().instance().set(&CAMPAIGN_MAP, &campaigns);
        env.storage().instance().set(&CAMPAIGN_COUNT, &count);

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
    pub fn get_campaign(env: Env, campaign_id: u64) -> Campaign {
        let campaigns: Map<u64, Campaign> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_MAP)
            .unwrap_or_else(|| panic!("No campaigns found"));

        campaigns
            .get(campaign_id)
            .unwrap_or_else(|| panic!("Campaign not found"))
    }

    /// Update campaign status
    pub fn update_campaign_status(env: Env, campaign_id: u64, status: u32) {
        require_not_paused(&env);

        let mut campaigns: Map<u64, Campaign> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_MAP)
            .unwrap_or_else(|| panic!("No campaigns found"));

        let campaign = campaigns
            .get(campaign_id)
            .unwrap_or_else(|| panic!("Campaign not found"));

        let (id, owner, goal, deadline, old_status, created_at) = campaign;

        owner.require_auth();

        let updated_campaign: Campaign = (id, owner, goal, deadline, status, created_at);
        campaigns.set(campaign_id, updated_campaign);
        env.storage().instance().set(&CAMPAIGN_MAP, &campaigns);

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
    pub fn get_campaign_count(env: Env) -> u64 {
        env.storage().instance().get(&CAMPAIGN_COUNT).unwrap_or(0)
    }

    /// Get all campaigns (utility function for testing)
    pub fn get_all_campaigns(env: Env) -> Vec<Campaign> {
        let campaigns: Map<u64, Campaign> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_MAP)
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let keys = campaigns.keys();

        for key in keys {
            if let Some(campaign) = campaigns.get(key) {
                result.push_back(campaign);
            }
        }

        result
    }

    /// Update raised amount for a campaign (can be called by other contracts)
    pub fn update_raised_amount(env: Env, campaign_id: u64, amount: i128) {
        require_not_paused(&env);

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let campaigns: Map<u64, Campaign> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_MAP)
            .unwrap_or_else(|| panic!("No campaigns found"));

        if !campaigns.contains_key(campaign_id) {
            panic!("Campaign not found");
        }

        let mut raised_amounts: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_RAISED)
            .unwrap_or(Map::new(&env));

        let current_raised: i128 = raised_amounts.get(campaign_id).unwrap_or(0);
        raised_amounts.set(campaign_id, current_raised + amount);
        env.storage().instance().set(&CAMPAIGN_RAISED, &raised_amounts);
    }

    /// Get raised amount for a campaign
    pub fn get_raised_amount(env: Env, campaign_id: u64) -> i128 {
        let raised_amounts: Map<u64, i128> = env
            .storage()
            .instance()
            .get(&CAMPAIGN_RAISED)
            .unwrap_or(Map::new(&env));

        raised_amounts.get(campaign_id).unwrap_or(0)
    }
}
