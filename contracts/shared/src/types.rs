use soroban_sdk::{Address, contracttype};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,
    Completed,
    Suspended,
    Rejected,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub id: u64,
    pub owner: Address,
    pub goal: i128,
    pub raised: i128,
    pub status: CampaignStatus,
    pub deadline: u64,
    pub asset_contract_id: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Donation {
    pub donor: Address,
    pub campaign_id: u64,
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Withdrawal {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub approved: bool,
}
