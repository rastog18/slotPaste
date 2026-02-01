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

/// When true, show chooser window ~800ms after startup to confirm visibility.
const STARTUP_SHOW_TEST: bool = true;

static CURRENT_TOKEN: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn current_token() -> &'static Mutex<Option<String>> {
    CURRENT_TOKEN.get_or_init(|| Mutex::new(None))
}

fn send_to_agent(msg: &str) {
    if let Ok(sock) = UdpSocket::bind("127.0.0.1:0") {
        let _ = sock.send_to(msg.as_bytes(), (BIND_ADDR, AGENT_PORT));
    }
}

/// Log all webview windows: count, and each label + is_visible().
fn log_windows(handle: &tauri::AppHandle) {
    let windows = handle.webview_windows();
    eprintln!("[clip-ui] webview windows count: {}", windows.len());
    for (label, win) in windows.iter() {
        let visible = win.is_visible().unwrap_or(false);
        eprintln!("[clip-ui]   window label={:?} is_visible={}", label, visible);
    }
}

/// One-time monitor diagnostics: log primary + all monitors (position, size, scale).
fn log_monitors(handle: &tauri::AppHandle) {
    eprintln!("[clip-ui] --- monitor diagnostics ---");
    if let Ok(Some(primary)) = handle.primary_monitor() {
        eprintln!("[clip-ui] primary_monitor: position={:?} size={:?} scale_factor={:?}",
            primary.position(), primary.size(), primary.scale_factor());
    } else {
        eprintln!("[clip-ui] primary_monitor: none or error");
    }
    if let Ok(monitors) = handle.available_monitors() {
        eprintln!("[clip-ui] available_monitors count: {}", monitors.len());
        for (i, m) in monitors.iter().enumerate() {
            eprintln!("[clip-ui]   monitor[{}]: position={:?} size={:?} scale_factor={:?}",
                i, m.position(), m.size(), m.scale_factor());
        }
    } else {
        eprintln!("[clip-ui] available_monitors: error");
    }
    eprintln!("[clip-ui] --- end monitor diagnostics ---");
}

fn show_chooser_on_main_thread(
    handle: tauri::AppHandle,
    mode: String,
    token: String,
    timeout_ms: u64,
) {
    eprintln!("[clip-ui] show_chooser_on_main_thread called mode={} token={} timeout_ms={}", mode, token, timeout_ms);
    let h = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        eprintln!("[clip-ui] show_chooser_on_main_thread: main thread closure entered");
        log_windows(&h);

        if h.get_webview_window("main").is_none() {
            eprintln!("[clip-ui] ERROR: window 'main' not found");
        }

        if let Ok(mut t) = current_token().lock() {
            *t = Some(token.clone());
        }
        let _ = h.emit("chooser-show", serde_json::json!({
            "mode": mode,
            "token": token,
            "timeout_ms": timeout_ms
        }));

        let chooser = h.get_webview_window("chooser");
        if chooser.is_none() {
            eprintln!("[clip-ui] ERROR: window 'chooser' not found");
            return;
        }
        eprintln!("[clip-ui] UDP show: got chooser window, applying show sequence");
        if let Some(win) = chooser {
            eprintln!("[clip-ui]   set_always_on_top(true): {:?}", win.set_always_on_top(true));
            eprintln!("[clip-ui]   set_visible_on_all_workspaces(true): {:?}", win.set_visible_on_all_workspaces(true));
            eprintln!("[clip-ui]   unminimize: {:?}", win.unminimize());
            eprintln!("[clip-ui]   hide: {:?}", win.hide());
            eprintln!("[clip-ui]   show: {:?}", win.show());
            let size_phys = tauri::PhysicalSize { width: 600, height: 250 };
            let pos_phys = tauri::PhysicalPosition { x: 20, y: 20 };
            eprintln!("[clip-ui]   set_size(Physical 600x250): {:?}", win.set_size(tauri::Size::Physical(size_phys)));
            eprintln!("[clip-ui]   set_position(Physical 20,20): {:?}", win.set_position(tauri::Position::Physical(pos_phys)));
            eprintln!("[clip-ui]   set_focus: {:?}", win.set_focus());
            if let Ok(pos) = win.outer_position() {
                eprintln!("[clip-ui]   outer_position() after set: {:?}", pos);
            }
            if let Ok(sz) = win.inner_size() {
                eprintln!("[clip-ui]   inner_size() after set: {:?}", sz);
            }
            eprintln!("[clip-ui]   after show sequence, is_visible: {:?}", win.is_visible());
        }
        eprintln!("[clip-ui] UDP show: done");
        log_windows(&h);
    });
}

#[tauri::command]
fn send_chosen(token: String, slot: u8) {
    eprintln!("[clip-ui] send_chosen: token={} slot={} -> UDP 45455", token, slot);
    let msg = format!(r#"{{"type":"chosen","token":"{}","slot":{}}}"#, token, slot);
    send_to_agent(&msg);
    if let Ok(mut t) = current_token().lock() {
        *t = None;
    }
}

#[tauri::command]
fn send_cancel(token: String, reason: String) {
    eprintln!("[clip-ui] send_cancel: token={} reason={} -> UDP 45455", token, reason);
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
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                eprintln!("[clip-ui] activation policy set to Accessory");
            }

            let handle = app.handle().clone();

            // Startup visibility test: show chooser after ~800ms (chooser is from config).
            if STARTUP_SHOW_TEST {
                let handle_test = handle.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(800));
                    let h = handle_test.clone();
                    let _ = handle_test.run_on_main_thread(move || {
                        eprintln!("[clip-ui] startup test: begin");
                        log_monitors(&h);
                        log_windows(&h);
                        let chooser = h.get_webview_window("chooser");
                        if chooser.is_none() {
                            eprintln!("[clip-ui] ERROR: startup test - window 'chooser' not found");
                            return;
                        }
                        eprintln!("[clip-ui] startup test: got chooser, applying show sequence (no center)");
                        if let Some(win) = chooser {
                            eprintln!("[clip-ui]   set_always_on_top(true): {:?}", win.set_always_on_top(true));
                            eprintln!("[clip-ui]   set_visible_on_all_workspaces(true): {:?}", win.set_visible_on_all_workspaces(true));
                            eprintln!("[clip-ui]   unminimize: {:?}", win.unminimize());
                            eprintln!("[clip-ui]   hide: {:?}", win.hide());
                            eprintln!("[clip-ui]   show: {:?}", win.show());
                            let size_phys = tauri::PhysicalSize { width: 600, height: 250 };
                            let pos_phys = tauri::PhysicalPosition { x: 20, y: 20 };
                            eprintln!("[clip-ui]   set_size(Physical 600x250): {:?}", win.set_size(tauri::Size::Physical(size_phys)));
                            eprintln!("[clip-ui]   set_position(Physical 20,20): {:?}", win.set_position(tauri::Position::Physical(pos_phys)));
                            eprintln!("[clip-ui]   set_focus: {:?}", win.set_focus());
                            if let Ok(pos) = win.outer_position() {
                                eprintln!("[clip-ui]   outer_position() after set: {:?}", pos);
                            }
                            if let Ok(sz) = win.inner_size() {
                                eprintln!("[clip-ui]   inner_size() after set: {:?}", sz);
                            }
                            eprintln!("[clip-ui]   after show sequence, is_visible: {:?}", win.is_visible());
                        }
                        eprintln!("[clip-ui] startup test: done (window left visible, no auto-hide)");
                        log_windows(&h);
                    });
                });
            }

            // UDP listener
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
                            eprintln!("[clip-ui] UDP recv {} bytes", n);
                            let s = match std::str::from_utf8(&buf[..n]) {
                                Ok(x) => x.trim(),
                                Err(_) => {
                                    eprintln!("[clip-ui] UDP recv: invalid UTF-8, skip");
                                    continue;
                                }
                            };
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                                let typ = v.get("type").and_then(|t| t.as_str());
                                eprintln!("[clip-ui] UDP parsed JSON type={:?}", typ);
                                if typ == Some("show") {
                                    if let (Some(mode), Some(token), Some(timeout_ms)) = (
                                        v.get("mode").and_then(|m| m.as_str()),
                                        v.get("token").and_then(|t| t.as_str()),
                                        v.get("timeout_ms").and_then(|t| t.as_u64()),
                                    ) {
                                        eprintln!(
                                            "[clip-ui] UDP show received mode={} token={} -> calling show_chooser_on_main_thread",
                                            mode, token
                                        );
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
                                                    if let Some(win) =
                                                        hide_handle.get_webview_window("chooser")
                                                    {
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
