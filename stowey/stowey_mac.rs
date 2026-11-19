//! Stowey Voice: what only macOS can do, asked of AppKit directly.
//!
//! Two things live here. The glass behind the pill, and the handover from an
//! older copy of the app that is still running under its old name.
//!
//! The overlay is a web view in a transparent panel, and a web view cannot
//! blur what lies behind its own window. So the glass is a native view, placed
//! under the web view exactly where the pill is. On macOS 26 that view is
//! Apple's own glass. On older systems it is the frosted material macOS has
//! had for years.
//!
//! The pill wears the opposite of the Mac's mode: dark glass in light mode,
//! light glass in dark mode, so it stands out from what is around it.
//!
//! How see through the glass is cannot be judged on a build machine: it runs
//! in a virtual machine that draws every glass flat, its own Dock included.
//! So the look can be tuned on a real Mac without a new build. A file named
//! `pill.txt` beside the app's log may hold two lines, for example
//! `glass = clear` and `tint = 0.2`. It is read again about once a second.
//!
//! No Tauri in this file, so it can be checked against AppKit on its own.

use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CStr};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, NSObjectProtocol};
use objc2::{msg_send, sel, MainThreadMarker};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSRunningApplication, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow, NSWindowOrderingMode,
};
use objc2_foundation::{ns_string, NSBundle, NSPoint, NSRect, NSSize, NSUserDefaults};

/// Where the pill is inside the overlay's web view: points from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    /// 0 hides the glass, 1 shows it in full. The pill fades, the glass with it.
    pub alpha: f64,
}

/// Which glass this Mac can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Apple's glass, from macOS 26.
    Liquid,
    /// The frosted material of earlier systems.
    Frosted,
}

const LIQUID_CLASS: &CStr = c"NSGlassEffectView";

/// Apple makes two kinds of its glass. Regular is frosted. Clear lets far more
/// through, and is the default here, because see through is what was asked for.
const CLEAR: bool = true;

/// How strongly the glass is tinted toward black or white, from 0 to 1.
/// Enough for the bars to read on any picture, little enough to stay glass.
const TINT: f64 = 0.2;

/// The file beside the log in which the look can be tuned.
const LOOK_FILE: &str = "pill.txt";

/// For the build machine's photographs. STOWEY_GLASS picks the glass,
/// "regular", "clear" or "frosted", and STOWEY_TINT its tint. Empty counts
/// as not set.
fn asked(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// What the glass wears: its tone, its kind and its tint.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    dark: bool,
    clear: bool,
    tint: f64,
}

/// The kind and the tint: the defaults, then the file, then the build
/// machine's switches. Anything unreadable is passed over.
fn chosen() -> (bool, f64) {
    let mut clear = CLEAR;
    let mut tint = TINT;
    let mut read = |key: &str, value: &str| match key {
        "glass" => match value {
            "clear" => clear = true,
            "regular" => clear = false,
            _ => {}
        },
        "tint" => {
            if let Ok(strength) = value.parse::<f64>() {
                tint = strength.clamp(0.0, 0.9);
            }
        }
        _ => {}
    };
    let file = std::env::var("HOME").ok().and_then(|home| {
        let identifier = NSBundle::mainBundle().bundleIdentifier()?.to_string();
        let path = format!("{home}/Library/Logs/{identifier}/{LOOK_FILE}");
        std::fs::read_to_string(path).ok()
    });
    for line in file.as_deref().unwrap_or("").lines() {
        if let Some((key, value)) = line.split_once('=') {
            read(key.trim(), value.trim());
        }
    }
    if let Some(value) = asked("STOWEY_GLASS") {
        read("glass", &value);
    }
    if let Some(value) = asked("STOWEY_TINT") {
        read("tint", &value);
    }
    (clear, tint)
}

/// Asked of the system itself instead of a version number: the class is
/// either there or it is not. Safe from any thread.
pub fn kind() -> Kind {
    if asked("STOWEY_GLASS").as_deref() == Some("frosted") {
        return Kind::Frosted;
    }
    if AnyClass::get(LIQUID_CLASS).is_some() {
        Kind::Liquid
    } else {
        Kind::Frosted
    }
}

enum Glass {
    Liquid(Retained<NSGlassEffectView>),
    Frosted(Retained<NSVisualEffectView>),
}

impl Glass {
    fn new(mtm: MainThreadMarker) -> Self {
        match kind() {
            Kind::Liquid => Glass::Liquid(NSGlassEffectView::new(mtm)),
            Kind::Frosted => {
                let view = NSVisualEffectView::new(mtm);
                // Follows light and dark mode, and lets the most through.
                view.setMaterial(NSVisualEffectMaterial::Popover);
                view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
                // The overlay never becomes the active window. Without this
                // the material would go flat and grey.
                view.setState(NSVisualEffectState::Active);
                Glass::Frosted(view)
            }
        }
    }

    fn view(&self) -> &NSView {
        match self {
            Glass::Liquid(view) => view,
            Glass::Frosted(view) => view,
        }
    }

    /// Dark glass or light glass, clear or frosted, tinted or not. The view
    /// is given a mode of its own, so it no longer follows the Mac's.
    fn dress(&self, look: Look) {
        // SAFETY: both names are constants AppKit sets up before our code runs.
        let name = unsafe {
            if look.dark {
                NSAppearanceNameDarkAqua
            } else {
                NSAppearanceNameAqua
            }
        };
        let appearance = NSAppearance::appearanceNamed(name);
        match self {
            Glass::Liquid(view) => {
                view.setAppearance(appearance.as_deref());
                view.setStyle(if look.clear {
                    NSGlassEffectViewStyle::Clear
                } else {
                    NSGlassEffectViewStyle::Regular
                });
                if look.tint > 0.0 {
                    let white = if look.dark { 0.0 } else { 1.0 };
                    view.setTintColor(Some(&NSColor::colorWithWhite_alpha(white, look.tint)));
                } else {
                    view.setTintColor(None);
                }
            }
            Glass::Frosted(view) => view.setAppearance(appearance.as_deref()),
        }
    }

    fn round(&self, radius: f64) {
        match self {
            Glass::Liquid(view) => view.setCornerRadius(radius),
            Glass::Frosted(view) => {
                // AppKit documents no way to round this view. This call is
                // the one Tauri's own vibrancy uses. Asked for first, so a
                // system without it gets square corners instead of a crash.
                if view.respondsToSelector(sel!(setCornerRadius:)) {
                    // SAFETY: the view answers this selector, which takes one CGFloat.
                    let _: () = unsafe { msg_send![&**view, setCornerRadius: radius] };
                }
            }
        }
    }
}

thread_local! {
    /// One glass for the one overlay. Main thread only, like all of AppKit.
    static GLASS: RefCell<Option<Glass>> = const { RefCell::new(None) };
    /// What the glass wears now, if anything yet.
    static WORN: Cell<Option<Look>> = const { Cell::new(None) };
    /// The kind and tint last read, and when: the file is not read every frame.
    static CHOSEN: Cell<Option<(Instant, bool, f64)>> = const { Cell::new(None) };
}

/// What the glass should wear right now. The tone is asked every time, so
/// the pill follows when the Mac changes mode. The file is read at most once
/// a second.
fn look() -> Look {
    let (clear, tint) = CHOSEN.with(|cell| match cell.get() {
        Some((read, clear, tint)) if read.elapsed() < Duration::from_secs(1) => (clear, tint),
        _ => {
            let (clear, tint) = chosen();
            cell.set(Some((Instant::now(), clear, tint)));
            (clear, tint)
        }
    });
    Look {
        dark: !system_is_dark(),
        clear,
        tint,
    }
}

/// Is the Mac itself in dark mode right now? Asked of the user's own setting
/// and not of this app: the app has a theme of its own, and once that is set,
/// every view in it reports the app's theme instead of the Mac's.
/// Safe from any thread.
pub fn system_is_dark() -> bool {
    NSUserDefaults::standardUserDefaults()
        .stringForKey(ns_string!("AppleInterfaceStyle"))
        .is_some_and(|style| style.to_string() == "Dark")
}

/// Puts the glass where the pill is. `ns_window` is the overlay's NSWindow.
/// Does nothing off the main thread.
pub fn place(ns_window: *mut c_void, frame: Frame) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if ns_window.is_null() {
        return;
    }
    // SAFETY: Tauri hands out the pointer of its own live window, and AppKit
    // objects are only touched here, on the main thread.
    let window: &NSWindow = unsafe { &*ns_window.cast::<NSWindow>() };
    let Some(content) = window.contentView() else {
        return;
    };
    GLASS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let glass = slot.get_or_insert_with(|| Glass::new(mtm));
        let view = glass.view();

        // Under the web view, so the pill's bars and labels draw on top of it.
        // SAFETY: the parent is only compared, never kept.
        let parent = unsafe { view.superview() };
        let attached =
            parent.is_some_and(|parent| Retained::as_ptr(&parent) == Retained::as_ptr(&content));
        if !attached {
            content.addSubview_positioned_relativeTo(view, NSWindowOrderingMode::Below, None);
        }

        let look = look();
        if WORN.get() != Some(look) {
            glass.dress(look);
            WORN.set(Some(look));
        }

        let visible = frame.alpha > 0.01 && frame.width >= 1.0 && frame.height >= 1.0;
        view.setHidden(!visible);
        if !visible {
            return;
        }
        // The web measures from the top, AppKit from the bottom unless the
        // view says otherwise.
        let bounds = content.bounds();
        let y = if content.isFlipped() {
            frame.y
        } else {
            bounds.size.height - frame.y - frame.height
        };
        view.setFrame(NSRect::new(
            NSPoint::new(frame.x, y),
            NSSize::new(frame.width, frame.height),
        ));
        view.setAlphaValue(frame.alpha.clamp(0.0, 1.0));
        glass.round(frame.radius.clamp(0.0, frame.height / 2.0));
    });
}

/// A copy of this app under an older name may still be running. It holds the
/// shortcut and answers in place of this one, so a new install would seem to
/// change nothing. Ask every such copy to quit and give it a moment.
///
/// A second start of this very app is left alone: that is for the single
/// instance guard, which brings the running one to the front.
pub fn retire_older_copies() {
    let Some(identifier) = NSBundle::mainBundle().bundleIdentifier() else {
        return;
    };
    let me = NSRunningApplication::currentApplication();
    let my_place = me.bundleURL().and_then(|url| url.path());
    let older = || {
        NSRunningApplication::runningApplicationsWithBundleIdentifier(&identifier)
            .iter()
            .filter(|app| app.processIdentifier() != me.processIdentifier())
            .filter(|app| app.bundleURL().and_then(|url| url.path()) != my_place)
            .filter(|app| !app.isTerminated())
            .collect::<Vec<_>>()
    };
    let found = older();
    if found.is_empty() {
        return;
    }
    for app in &found {
        app.terminate();
    }
    let asked = Instant::now();
    while asked.elapsed() < Duration::from_secs(4) && !older().is_empty() {
        std::thread::sleep(Duration::from_millis(100));
    }
}
