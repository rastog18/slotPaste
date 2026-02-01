//! Slotpaste chooser UI: UDP listener for agent, overlay window, send chosen/cancel to agent.

use std::net::UdpSocket;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tauri::Manager;

const UI_PORT: u16 = 45454;
const AGENT_PORT: u16 = 45455;
const BIND_ADDR: &str = "127.0.0.1";

static CURRENT_TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn current_token() -> &'static Mutex<Option<String>> {
    CURRENT_TOKEN.get_or_init(|| Mutex::new(None))
}

fn send_to_agent(msg: &str) {
    if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
        let _ = sock.send_to(msg.as_bytes(), (BIND_ADDR, AGENT_PORT));
    }
}

fn show_chooser_on_main_thread(
    handle: tauri::AppHandle,
    mode: String,
    token: String,
    timeout_ms: u64,
) {
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        if let Ok(mut t) = current_token().lock() {
            *t = Some(token.clone());
        }
        let _ = h.emit("chooser-show", serde_json::json!({
            "mode": mode,
            "token": token,
            "timeout_ms": timeout_ms
        }));
        if let Some(win) = h.get_webview_window("chooser") {
            let _ = win.center();
            let _ = win.show();
            let _ = win.set_focus();
        }
    });
}

#[tauri::command]
fn send_chosen(token: String, slot: u8) {
    let msg = format!(r#"{{"type":"chosen","token":"{}","slot":{}}}"#, token, slot);
    send_to_agent(&msg);
    if let Ok(mut t) = current_token().lock() {
        *t = None;
    }
}

#[tauri::command]
fn send_cancel(token: String, reason: String) {
    let msg = format!(r#"{{"type":"cancel","token":"{}","reason":"{}"}}"#, token, reason);
    send_to_agent(&msg);
    if let Ok(mut t) = current_token().lock() {
        *t = None;
    }
}

#[tauri::command]
fn hide_chooser(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("chooser") {
        let _ = win.hide();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let handle = app.handle().clone();
            thread::spawn(move || {
                let sock = match UdpSocket::bind((BIND_ADDR, UI_PORT)) {
                    Ok(s) => {
                        eprintln!("[clip-ui] UDP listening on {}:{}", BIND_ADDR, UI_PORT);
                        s
                    }
                    Err(e) => {
                        eprintln!("[clip-ui] UDP bind failed: {} (is port {} in use?)", e, UI_PORT);
                        return;
                    }
                };
                sock.set_read_timeout(Some(Duration::from_millis(500))).ok();
                let mut buf = [0u8; 512];
                loop {
                    match sock.recv_from(&mut buf) {
                        Ok((n, _)) => {
                            let s = match std::str::from_utf8(&buf[..n]) {
                                Ok(x) => x.trim(),
                                Err(_) => continue,
                            };
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                                if v.get("type").and_then(|t| t.as_str()) == Some("show") {
                                    if let (Some(mode), Some(token), Some(timeout_ms)) = (
                                        v.get("mode").and_then(|m| m.as_str()),
                                        v.get("token").and_then(|t| t.as_str()),
                                        v.get("timeout_ms").and_then(|t| t.as_u64()),
                                    ) {
                                        eprintln!("[clip-ui] UDP show received mode={} token={}", mode, token);
                                        show_chooser_on_main_thread(
                                            handle.clone(),
                                            mode.to_string(),
                                            token.to_string(),
                                            timeout_ms,
                                        );
                                    }
                                } else if v.get("type").and_then(|t| t.as_str()) == Some("hide") {
                                    if let Some(token) = v.get("token").and_then(|t| t.as_str()) {
                                        if let Ok(guard) = current_token().lock() {
                                            if guard.as_deref() == Some(token) {
                                                let hide_handle = handle.clone();
                                                let _ = handle.run_on_main_thread(move || {
                                                    if let Some(win) = hide_handle.get_webview_window("chooser") {
                                                        let _ = win.hide();
                                                    }
                                                    if let Ok(mut t) = current_token().lock() {
                                                        *t = None;
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![send_chosen, send_cancel, hide_chooser])
        .run(tauri::generate_context!())
        .expect("error running clip-ui");
}
