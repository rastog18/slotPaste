//! UDP IPC: agent sends show/hide to UI:45454; agent listens on 45455 for chosen/cancel.

use crate::state_machine::Event;
use std::net::UdpSocket;
use std::sync::mpsc::Sender;
use std::thread;
use tracing::{error, info, warn};

const UI_PORT: u16 = 45454;
const AGENT_PORT: u16 = 45455;
const BIND_ADDR: &str = "127.0.0.1";

/// Send show chooser to UI. Best-effort.
pub fn send_show(mode: &str, token: &str, timeout_ms: u64) {
    let msg = format!(
        r#"{{"type":"show","mode":"{}","token":"{}","timeout_ms":{},"anchor":"mouse"}}"#,
        mode, token, timeout_ms
    );
    info!("ipc: send_show -> {}:{} (mode={}, token={})", BIND_ADDR, UI_PORT, mode, token);
    if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
        match sock.send_to(msg.as_bytes(), (BIND_ADDR, UI_PORT)) {
            Ok(n) => info!("ipc: send_show sent {} bytes", n),
            Err(e) => info!("ipc: send_show failed: {} (UI may not be running)", e),
        }
    } else {
        info!("ipc: send_show failed to bind ephemeral socket");
    }
}

/// Send hide chooser to UI. Best-effort.
pub fn send_hide(token: &str) {
    let msg = format!(r#"{{"type":"hide","token":"{}"}}"#, token);
    if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
        let _ = sock.send_to(msg.as_bytes(), (BIND_ADDR, UI_PORT));
    }
}

/// Run listener on 127.0.0.1:45455; parse newline-delimited JSON and send ChooserChosen/ChooserCancel to tx.
pub fn start_response_listener(tx: Sender<Event>) {
    thread::spawn(move || {
        let sock = match UdpSocket::bind((BIND_ADDR, AGENT_PORT)) {
            Ok(s) => s,
            Err(e) => {
                error!("ipc listener bind failed: {}", e);
                return;
            }
        };
        let mut buf = [0u8; 1024];
        loop {
            match sock.recv_from(&mut buf) {
                Ok((n, _)) => {
                    let s = match std::str::from_utf8(&buf[..n]) {
                        Ok(x) => x.trim(),
                        Err(_) => continue,
                    };
                    for line in s.lines() {
                        if let Some(ev) = parse_response(line) {
                            if tx.send(ev).is_err() {
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("ipc recv error: {}", e);
                }
            }
        }
    });
}

fn parse_response(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let typ = v.get("type")?.as_str()?;
    let token = v.get("token")?.as_str()?.to_string();
    match typ {
        "chosen" => {
            let slot = v.get("slot")?.as_u64()? as u8;
            if (1..=6).contains(&slot) {
                info!("ipc: received from UI -> ChooserChosen token={} slot={}", token, slot);
                return Some(Event::ChooserChosen { token, slot_num: slot });
            }
        }
        "cancel" => {
            let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("timeout").to_string();
            info!("ipc: received from UI -> ChooserCancel token={} reason={}", token, reason);
            return Some(Event::ChooserCancel { token, reason });
        }
        _ => {}
    }
    None
}
