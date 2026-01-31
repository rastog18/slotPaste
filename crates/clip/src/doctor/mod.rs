//! Doctor subcommand: system configuration checks.

#[cfg(target_os = "macos")]
pub mod accessibility;

#[cfg(target_os = "macos")]
pub use accessibility::run_accessibility_check;

#[cfg(not(target_os = "macos"))]
pub fn run_accessibility_check() {
    println!("Accessibility check is only supported on macOS.");
    println!("On this platform, Slotpaste does not require Accessibility permission.");
}
