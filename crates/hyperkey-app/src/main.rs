#![cfg_attr(not(test), windows_subsystem = "windows")]
//! Hyperkey: a tray-resident process that owns the keyboard hook engine and
//! shows a GPUI settings window on demand.
mod app;
mod controller;
mod copy;
mod platform;
mod settings_store;
mod ui;

use platform::{session_window, single_instance};

fn main() {
    // Only one Hyperkey may own the keyboard hook at a time; the name is
    // shared with the C# build so the two cannot run side by side either.
    let _single_instance = match single_instance::acquire() {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            // Ask the running instance to surface its settings window; fall
            // back to a visible message when the broadcast cannot be posted.
            if !single_instance::notify_existing() {
                session_window::message_box("Hyperkey is already running.", "Hyperkey");
            }
            return;
        }
        Err(error) => {
            session_window::message_box(&format!("Hyperkey could not start: {error}"), "Hyperkey");
            return;
        }
    };

    let start_to_tray = std::env::args().any(|arg| arg == "--tray");
    app::run(start_to_tray);
}
