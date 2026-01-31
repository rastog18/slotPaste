//! macOS Accessibility permission check.
//!
//! Uses AXIsProcessTrustedWithOptions (via macos-accessibility-client) to check
//! if the current process has Accessibility permission. We do NOT use the prompt
//! option; we guide the user to System Settings ourselves.

use macos_accessibility_client::accessibility::application_is_trusted;
use std::io::{self, Write};
use std::process::Command;

const SYSTEM_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

/// Runs the Accessibility check flow for macOS.
/// - If trusted: prints ✅ and exits.
/// - If not trusted: explains, offers to open Settings, loops until trusted or Ctrl+C.
pub fn run_accessibility_check() {
    if application_is_trusted() {
        println!("Accessibility: ✅ granted");
        return;
    }

    // Not trusted: print explanation
    println!("Accessibility: ❌ not granted");
    println!();
    println!("We need Accessibility permission to register global hotkeys / event tap for Slotpaste.");
    println!("No keystrokes are logged or sent anywhere; text stays local.");
    println!();

    // Offer to open System Settings
    print!("Would you like to open System Settings to grant permission? [Y/n] ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        print_manual_instructions();
        return;
    }

    let trimmed = input.trim().to_lowercase();
    if trimmed == "n" || trimmed == "no" {
        print_manual_instructions();
        return;
    }

    // Open System Settings
    if Command::new("open").arg(SYSTEM_SETTINGS_URL).output().is_err() {
        eprintln!("Failed to open System Settings.");
        print_manual_instructions();
        return;
    }

    println!("Opening System Settings...");
    println!();

    // Loop: prompt to re-check until trusted or Ctrl+C
    loop {
        print!("After granting permission, press Enter to re-check (Ctrl+C to exit). ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }

        if application_is_trusted() {
            println!("Accessibility: ✅ granted");
            return;
        }
    }
}

fn print_manual_instructions() {
    println!();
    println!("To enable manually:");
    println!("  1. Open System Settings → Privacy & Security → Accessibility");
    println!("  2. Add this app (or Terminal) to the list and enable it.");
    println!("  3. Run `clip doctor` again to verify.");
}

