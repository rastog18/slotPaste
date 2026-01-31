use std::time::Duration;
use tracing::{info, warn};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("agent running");

    ctrlc::set_handler(move || {
        warn!("agent stopping");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    loop {
        std::thread::sleep(Duration::from_secs(5));
        info!("agent running");
    }
}
