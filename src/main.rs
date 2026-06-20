pub mod config;
pub mod errors;

pub mod friendbot;
pub mod horizon;
pub mod services;
mod setup;
pub mod utils;

fn main() {
    if let Err(err) = config::Config::from_env() {
        eprintln!("Startup configuration error: {err}");
        std::process::exit(1);
    }
}