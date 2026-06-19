#![no_std]

use soroban_sdk::{Address, Env, Symbol, contract, contractimpl, contracttype, symbol_short, token};

const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");
const TOKEN_ID: Symbol = symbol_short!("TOKEN_ID");

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
pub struct WithdrawalContract;

#[contractimpl]
impl WithdrawalContract {
    /// Initialize withdrawal settings, token ID, and admin address
    pub fn initialize(env: Env, beneficiary: Address, max_withdrawal: i128, token_id: Address, admin: Address) {
        let key = Symbol::new(&env, "settings");
        env.storage()
            .instance()
            .set(&key, &(beneficiary, max_withdrawal));
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

    /// Withdraw funds from the contract — verifies balance then transfers XLM to beneficiary
    pub fn withdraw(env: Env, amount: i128) -> bool {
        require_not_paused(&env);

        let key = Symbol::new(&env, "settings");
        let (beneficiary, max_withdrawal): (Address, i128) = env
            .storage()
            .instance()
            .get(&key)
            .expect("withdrawal not initialized");

        beneficiary.require_auth();

        if amount > max_withdrawal {
            return false;
        }

        let token_id: Address = env
            .storage()
            .instance()
            .get(&TOKEN_ID)
            .unwrap_or_else(|| panic!("Token ID not set. Call initialize() first."));

        let token_client = token::Client::new(&env, &token_id);
        let contract_balance = token_client.balance(&env.current_contract_address());

        if contract_balance < amount {
            return false;
        }

        token_client.transfer(&env.current_contract_address(), &beneficiary, &amount);

        let withdrawn_key = Symbol::new(&env, "total_withdrawn");
        let total: i128 = env.storage().instance().get(&withdrawn_key).unwrap_or(0);
        env.storage()
            .instance()
            .set(&withdrawn_key, &(total + amount));

        true
    }

    /// Get total withdrawn
    pub fn get_total_withdrawn(env: Env) -> i128 {
        let key = Symbol::new(&env, "total_withdrawn");
        env.storage().instance().get(&key).unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use soroban_sdk::{Address, Env, testutils::Address as _, token::Client as TokenClient};
    use crate::{WithdrawalContract, WithdrawalContractClient};

    fn setup(env: &Env, max_withdrawal: i128) -> (WithdrawalContractClient, Address, Address, Address) {
        let contract_id = env.register_contract(None, WithdrawalContract);
        let client = WithdrawalContractClient::new(env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(Address::generate(env)).address();
        let beneficiary = Address::generate(env);
        let admin = Address::generate(env);

        client.initialize(&beneficiary, &max_withdrawal, &token_id, &admin);

        (client, token_id, beneficiary, admin)
    }

    #[test]
    fn test_withdraw_success() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, beneficiary, _admin) = setup(&env, 500i128);

        // Fund the contract
        TokenClient::new(&env, &token_id).mint(&client.address, &300i128);

        assert!(client.withdraw(&200i128));
        assert_eq!(client.get_total_withdrawn(), 200i128);

        // Beneficiary should have received the tokens
        assert_eq!(TokenClient::new(&env, &token_id).balance(&beneficiary), 200i128);
    }

    #[test]
    fn test_withdraw_exceeds_max() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _beneficiary, _admin) = setup(&env, 100i128);

        TokenClient::new(&env, &token_id).mint(&client.address, &500i128);

        // amount > max_withdrawal → returns false, no transfer
        assert!(!client.withdraw(&200i128));
        assert_eq!(client.get_total_withdrawn(), 0i128);
    }

    #[test]
    fn test_withdraw_insufficient_contract_balance() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _beneficiary, _admin) = setup(&env, 500i128);

        // Only fund 50 tokens but try to withdraw 100
        TokenClient::new(&env, &token_id).mint(&client.address, &50i128);

        assert!(!client.withdraw(&100i128));
        assert_eq!(client.get_total_withdrawn(), 0i128);
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn test_withdraw_when_paused() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _beneficiary, admin) = setup(&env, 500i128);
        TokenClient::new(&env, &token_id).mint(&client.address, &300i128);

        client.pause(&admin);
        client.withdraw(&100i128);
    }

    #[test]
    fn test_pause_unpause_withdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, token_id, _beneficiary, admin) = setup(&env, 500i128);
        TokenClient::new(&env, &token_id).mint(&client.address, &300i128);

        client.pause(&admin);
        client.unpause(&admin);

        assert!(client.withdraw(&100i128));
        assert_eq!(client.get_total_withdrawn(), 100i128);
    }
}
