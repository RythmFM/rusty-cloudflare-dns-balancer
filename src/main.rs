mod models;
mod health_checker;
mod config;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use log::{info, warn};
use cloudflare::framework::async_api::Client;
use cloudflare::framework::auth::Credentials;
use cloudflare::framework::{HttpApiClientConfig, Environment};
use std::env;
use crate::health_checker::HealthChecker;
use std::process::exit;
use tokio::time::Duration;
use crate::config::Config;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    env_logger::init();

    let version: Option<&str> = built_info::GIT_COMMIT_HASH;
    let dirty: Option<bool> = built_info::GIT_DIRTY;
    let profile: &str = built_info::PROFILE;
    let build_time: &str = built_info::BUILT_TIME_UTC;
    info!(
        "Starting rusty-cloudflare-dns-balancer with revision {} ({}), built with profile {} at {}",
        version.unwrap_or("{untagged build}"),
        if dirty.unwrap_or(true) {
            "dirty"
        } else {
            "clean"
        },
        profile,
        build_time
    );

    let token = env::var("CF_TOKEN").expect("Please provide a `CF_TOKEN` in env!");
    let client = Client::new(
        Credentials::UserAuthToken { token },
        HttpApiClientConfig::default(),
        Environment::Production,
    ).expect("Couldn't construct the CloudFlare API Client... Panic!");

    let service_data = env::var("SERVICE_TARGETS")
        .expect("Please provide a `SERVICE_TARGETS` in env!");
    let service_targets = Config::read_service_targets(service_data.as_str());

    let check_interval = env::var("CHECK_INTERVAL")
        .map(|str| str.parse().unwrap())
        .map(|dur| Duration::from_secs(dur))
        .unwrap_or(Duration::from_secs(30));

    let health_checker = HealthChecker::new(client, service_targets)
        .run(check_interval);

    tokio::select! {
        _val = health_checker => {
            warn!("Health checker task ended. Stopping service...");
            exit(1);
        }
    }
}

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}