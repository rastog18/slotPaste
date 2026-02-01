#[cfg(not(target_os = "macos"))]
use std::time::Duration;
use tracing::info;
#[cfg(not(target_os = "macos"))]
use tracing::warn;

mod ipc;
mod keys;
mod persistence;
mod state_machine;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("agent running");

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = run_macos() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        info!("Keyboard capture only supported on macOS");
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
}

#[cfg(target_os = "macos")]
fn run_macos() -> Result<(), String> {
    use crate::ipc::udp;
    use crate::state_machine::{run, Event};
    use std::sync::atomic::AtomicU8;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread;
    use tracing::warn;

    if !macos::event_tap::has_accessibility_permission() {
        return Err(
            "Accessibility permission not granted. Run `clip doctor` to open System Settings."
                .to_string(),
        );
    }

    let persistence = match persistence::sqlite::init_db() {
        Ok(conn) => {
            if let Ok(path) = persistence::sqlite::db_path() {
                info!("persistence: {}", path.display());
            } else {
                info!("persistence: enabled");
            }
            Some(conn)
        }
        Err(e) => {
            warn!("persistence init failed: {}, slots in-memory only", e);
            None
        }
    };

    let (tx, rx) = mpsc::channel();
    let mode = Arc::new(AtomicU8::new(0));

    udp::start_response_listener(tx.clone());

    ctrlc::set_handler({
        let tx = tx.clone();
        move || {
            tracing::warn!("agent stopping");
            let _ = tx.send(Event::Quit);
            core_foundation::runloop::CFRunLoop::get_main().stop();
        }
    })
    .map_err(|e| format!("Failed to set Ctrl+C handler: {}", e))?;

    let state_tx = tx.clone();
    let mode_state = mode.clone();
    let state_handle = thread::spawn(move || run(rx, state_tx, mode_state, persistence));

    macos::event_tap::run_event_tap_with_sender(tx)?;

    let _ = state_handle.join();
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos;
