#![no_std]

use soroban_sdk::{Address, Env, Symbol, contract, contractimpl, contracttype, symbol_short};

const PAUSED: Symbol = symbol_short!("PAUSED");
const ADMIN: Symbol = symbol_short!("ADMIN");

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
    /// Initialize withdrawal settings and set admin address
    pub fn initialize(env: Env, beneficiary: Address, max_withdrawal: i128, admin: Address) {
        let key = Symbol::new(&env, "settings");
        env.storage()
            .instance()
            .set(&key, &(beneficiary, max_withdrawal));
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

    /// Withdraw funds from the contract
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
