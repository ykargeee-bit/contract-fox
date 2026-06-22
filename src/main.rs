pub mod config;
pub mod db;
pub mod errors;
pub mod models;

pub mod friendbot;
pub mod horizon;
pub mod services;
mod setup;
pub mod utils;
pub mod webhooks;

fn main() {
    if let Err(err) = config::Config::from_env() {
        eprintln!("Startup configuration error: {err}");
        std::process::exit(1);
    }
}
