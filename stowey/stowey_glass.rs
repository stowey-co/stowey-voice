//! Stowey Voice: tells macOS where the pill is, so it can put glass behind it.
//!
//! The overlay's web view measures the pill and calls `stowey_glass` whenever
//! it moves, grows or fades. The glass itself is native and lives in
//! `stowey_mac.rs`. On other systems these calls do nothing and the pill is
//! drawn solid.

use serde::Deserialize;
use specta::Type;
use std::time::{Duration, Instant};
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
#[path = "stowey_mac.rs"]
mod mac;

/// Where the pill is inside the overlay's web view: points from its top left.
#[derive(Clone, Copy, Debug, Deserialize, Type)]
pub struct GlassFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    /// 0 hides the glass, 1 shows it in full.
    pub alpha: f64,
}

/// Which glass this machine can draw: "liquid" is Apple's own, from macOS 26.
/// "frosted" is the blur of earlier systems. "none" means the pill is solid.
/// The overlay's stylesheet reads it to know how much to draw itself.
#[tauri::command]
#[specta::specta]
pub fn stowey_glass_kind() -> String {
    #[cfg(target_os = "macos")]
    let kind = match mac::kind() {
        mac::Kind::Liquid => "liquid",
        mac::Kind::Frosted => "frosted",
    };
    #[cfg(not(target_os = "macos"))]
    let kind = "none";
    kind.to_string()
}

/// Puts the glass where the pill now is. Answers with what the pill wears,
/// "dark" or "light", so the web view can colour the bars to match. It is the
/// opposite of the Mac's mode.
#[tauri::command]
#[specta::specta]
pub fn stowey_glass(app: AppHandle, frame: GlassFrame) -> String {
    #[cfg(target_os = "macos")]
    let tone = {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(window) = handle.get_webview_window("recording_overlay") else {
                return;
            };
            let Ok(ns_window) = window.ns_window() else {
                return;
            };
            mac::place(
                ns_window,
                mac::Frame {
                    x: frame.x,
                    y: frame.y,
                    width: frame.width,
                    height: frame.height,
                    radius: frame.radius,
                    alpha: frame.alpha,
                },
            );
        });
        if mac::system_is_dark() {
            "light"
        } else {
            "dark"
        }
    };
    #[cfg(not(target_os = "macos"))]
    let tone = {
        let _ = (app, frame);
        "dark"
    };
    tone.to_string()
}

/// Called first thing when the app starts. A copy of the app under an older
/// name that is still running would answer in place of this one.
pub fn retire_older_copies() {
    #[cfg(target_os = "macos")]
    mac::retire_older_copies();
}

/// A hidden switch for the build machine. With STOWEY_SHOW_PILL set, the pill
/// appears a few seconds after the start, with made up levels and without a
/// recording, so that a screenshot can be taken of the real thing.
/// "working" shows the pill as it is while the text is being made.
pub fn demo(app: &AppHandle) {
    let Ok(wanted) = std::env::var("STOWEY_SHOW_PILL") else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        log::info!("Stowey Voice: showing the pill for a screenshot ({wanted})");
        if wanted == "working" {
            crate::overlay::show_transcribing_overlay(&app);
            std::thread::sleep(Duration::from_secs(60));
        } else {
            crate::overlay::show_recording_overlay(&app);
            std::thread::sleep(Duration::from_millis(400));
            crate::overlay::emit_recording_ready(&app);
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(60) {
                let t = started.elapsed().as_secs_f32();
                let levels: Vec<f32> = (0..16)
                    .map(|bar| 0.15 + 0.7 * (t * 5.0 + bar as f32 * 1.3).sin().abs())
                    .collect();
                crate::overlay::emit_levels(&app, &levels);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        crate::overlay::hide_recording_overlay(&app);
    });
}
