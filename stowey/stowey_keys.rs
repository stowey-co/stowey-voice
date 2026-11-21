//! Stowey Voice: one shortcut that copies the last dictation.
//!
//! Fn and C by default. It puts the text of the last dictation on the
//! clipboard, the text as it was typed, so it can be pasted again anywhere.
//! The keys can be changed in Settings like every other shortcut.
//!
//! The hard part is the Fn key. Many people dictate by holding Fn alone, and
//! then Fn and C begins with the very key that starts a dictation: the
//! recording is already running when C arrives. So the shortcut does not act
//! on its own. It goes through the queue the dictation keys go through
//! (`TranscriptionCoordinator`), which knows what is going on, and `decide`
//! picks what to do:
//!
//! * nothing is going on: copy;
//! * a recording began a moment ago on a key this shortcut contains: that was
//!   the Fn of this shortcut. Throw the recording away, then copy;
//! * a recording has run for a while on such a key and the key is still down:
//!   the C was a slip of the finger. Leave the dictation alone;
//! * a recording runs on keys this shortcut does not contain: copy, the
//!   recording is none of its business;
//! * a dictation is being turned into text: wait until it is typed, then copy
//!   that one, since "the last dictation" now means the one in the making.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{info, warn};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::actions::ShortcutAction;
use crate::audio_feedback::{play_feedback_sound, SoundType};
use crate::managers::history::HistoryManager;
use crate::settings::ShortcutBinding;
use crate::TranscriptionCoordinator;

/// The id of the shortcut in the settings and in the action map.
pub const BINDING: &str = "copy_last_transcript";

/// Fn and C. macOS has its own use for this pair (Control Centre). The app
/// swallows the keys it listens for, and the pair can be changed in Settings.
const DEFAULT_KEYS: &str = "fn+c";

/// How long after the dictation key went down a C still counts as "Fn and C
/// pressed together". Past this, somebody is dictating.
const CHORD: Duration = Duration::from_millis(2000);

/// A copy asked for while a dictation was being typed is carried out when the
/// typing is done. If that never comes, the wish lapses.
const PATIENCE: Duration = Duration::from_secs(90);

/// The text of the last dictation exactly as it was typed, house style
/// applied. The history keeps the text as the models produced it, which is
/// not always the same.
static LAST_TYPED: Mutex<Option<String>> = Mutex::new(None);

/// Set when a copy has to wait for the dictation in the making.
static WAITING_SINCE: Mutex<Option<Instant>> = Mutex::new(None);

/// The shortcut as it appears in the settings.
pub fn binding() -> ShortcutBinding {
    ShortcutBinding {
        id: BINDING.to_string(),
        name: "Copy last dictation".to_string(),
        description: "Copies the text of your last dictation, so you can paste it again."
            .to_string(),
        default_binding: DEFAULT_KEYS.to_string(),
        current_binding: DEFAULT_KEYS.to_string(),
    }
}

/// What the copy shortcut does, given what the app is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Copy at once.
    Copy,
    /// The dictation key of this very shortcut started a recording: drop it,
    /// then copy.
    DropRecordingThenCopy,
    /// A dictation is on its way to the page: copy it once it is there.
    CopyWhenTyped,
    /// Somebody is dictating: do nothing.
    LeaveAlone,
}

/// A recording in progress, as the copy shortcut needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recording {
    /// How long ago the key that started it went down.
    pub since_key_down: Duration,
    /// It outlives its key: hands free, or toggle mode.
    pub hands_free: bool,
    /// Its dictation shortcut is made only of keys the copy shortcut also
    /// contains, as Fn is in Fn and C. Pressing the copy shortcut then sets
    /// the dictation off first.
    pub on_our_key: bool,
}

/// `typing`: a finished recording is being turned into text.
pub fn decide(recording: Option<Recording>, typing: bool) -> Decision {
    match recording {
        Some(rec) if !rec.on_our_key => Decision::Copy,
        Some(rec) if rec.since_key_down <= CHORD => Decision::DropRecordingThenCopy,
        // Hands free and begun long ago: its key is up, and pressing ours
        // again would have ended it before this point. Nothing to protect.
        Some(rec) if rec.hands_free => Decision::Copy,
        Some(_) => Decision::LeaveAlone,
        None if typing => Decision::CopyWhenTyped,
        None => Decision::Copy,
    }
}

/// Whether pressing `copy` unavoidably presses `dictation` first: every key of
/// the dictation shortcut is also in the copy shortcut. "fn" and "fn+c" do,
/// "option+space" and "fn+c" do not. Left and right variants count as the key.
pub fn sets_off(dictation: &str, copy: &str) -> bool {
    fn keys(shortcut: &str) -> Vec<String> {
        shortcut
            .split('+')
            .map(|key| key.trim().to_lowercase())
            .filter(|key| !key.is_empty())
            .map(|key| {
                let key = key
                    .strip_suffix("_left")
                    .or_else(|| key.strip_suffix("_right"))
                    .unwrap_or(&key)
                    .to_string();
                match key.as_str() {
                    "opt" | "alt" => "option".to_string(),
                    "cmd" | "meta" | "super" => "command".to_string(),
                    "control" => "ctrl".to_string(),
                    "function" => "fn".to_string(),
                    _ => key,
                }
            })
            .collect()
    }
    let dictation = keys(dictation);
    let copy = keys(copy);
    !dictation.is_empty() && dictation.len() < copy.len() && dictation.iter().all(|key| copy.contains(key))
}

/// What the coordinator knows when the copy shortcut is pressed.
pub struct Facts {
    /// The running recording: its shortcut id, how long ago its key went
    /// down, and whether it outlives the key.
    pub recording: Option<(String, Duration, bool)>,
    /// A finished recording is being turned into text.
    pub typing: bool,
    /// A dictation key pressed while that was going on, waiting its turn.
    pub waiting_key: Option<String>,
}

/// What the coordinator has to put right in its own books afterwards.
pub struct Verdict {
    pub decision: Decision,
    /// The key waiting its turn was the Fn of this shortcut: forget it, or a
    /// recording starts by itself once the text is typed.
    pub forget_waiting_key: bool,
}

/// Called by the coordinator, on its own thread, with the state it holds.
pub fn on_copy_key(app: &AppHandle, facts: Facts) -> Verdict {
    let settings = crate::settings::get_settings(app);
    let keys_of = |id: &str| {
        settings
            .bindings
            .get(id)
            .map(|binding| binding.current_binding.clone())
            .unwrap_or_default()
    };
    let ours = keys_of(BINDING);
    let recording = facts.recording.map(|(id, since_key_down, hands_free)| Recording {
        since_key_down,
        hands_free,
        on_our_key: sets_off(&keys_of(&id), &ours),
    });
    let decision = decide(recording, facts.typing);
    let forget_waiting_key = decision == Decision::CopyWhenTyped
        && facts
            .waiting_key
            .is_some_and(|id| sets_off(&keys_of(&id), &ours));
    match decision {
        Decision::Copy => {
            copy_and_tell(app);
        }
        Decision::DropRecordingThenCopy => {
            info!("Stowey Voice: the copy shortcut drops the recording its own key began");
            crate::utils::cancel_current_operation(app);
            copy_and_tell(app);
        }
        Decision::CopyWhenTyped => {
            info!("Stowey Voice: copy asked for mid dictation, waiting for the text");
            if let Ok(mut waiting) = WAITING_SINCE.lock() {
                *waiting = Some(Instant::now());
            }
        }
        Decision::LeaveAlone => {
            info!("Stowey Voice: copy shortcut ignored, a dictation is running");
        }
    }
    Verdict {
        decision,
        forget_waiting_key,
    }
}

/// Called by the coordinator each time a dictation has been dealt with.
pub fn typed(app: &AppHandle) {
    let asked = WAITING_SINCE.lock().ok().and_then(|mut waiting| waiting.take());
    if still_wanted(asked, Instant::now()) {
        copy_and_tell(app);
    }
}

fn still_wanted(asked: Option<Instant>, now: Instant) -> bool {
    asked.is_some_and(|at| now.saturating_duration_since(at) <= PATIENCE)
}

/// Called by the pipeline with the text it is about to type.
pub fn remember(text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if let Ok(mut last) = LAST_TYPED.lock() {
        *last = Some(text.to_string());
    }
}

/// Puts the last dictation on the clipboard. True when there was one.
pub fn copy_last(app: &AppHandle) -> bool {
    let typed = LAST_TYPED.lock().ok().and_then(|last| last.clone());
    if let Some(text) = typed {
        return match app.clipboard().write_text(text) {
            Ok(()) => {
                info!("Stowey Voice: last dictation copied");
                true
            }
            Err(err) => {
                warn!("Stowey Voice: could not copy the last dictation: {err}");
                false
            }
        };
    }
    // Nothing dictated since the app started: the history still has the last one.
    let history = app.state::<Arc<HistoryManager>>();
    match history.get_latest_completed_entry() {
        Ok(Some(_)) => {
            crate::tray::copy_last_transcript(app);
            true
        }
        Ok(None) => {
            info!("Stowey Voice: nothing to copy yet");
            false
        }
        Err(err) => {
            warn!("Stowey Voice: could not read the history: {err}");
            false
        }
    }
}

/// Copies, and plays the "done" sound when something was copied, since a
/// copy shows nothing on screen.
fn copy_and_tell(app: &AppHandle) {
    if copy_last(app) {
        play_feedback_sound(app, SoundType::Stop);
    }
}

/// The shortcut itself. It only hands the press to the coordinator.
pub struct CopyLast;

impl ShortcutAction for CopyLast {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut: &str) {
        match app.try_state::<TranscriptionCoordinator>() {
            Some(coordinator) => coordinator.stowey_copy(),
            None => {
                copy_and_tell(app);
            }
        }
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn rec(since_ms: u64, hands_free: bool, on_our_key: bool) -> Option<Recording> {
        Some(Recording {
            since_key_down: ms(since_ms),
            hands_free,
            on_our_key,
        })
    }

    #[test]
    fn copy_with_nothing_going_on_copies() {
        assert_eq!(decide(None, false), Decision::Copy);
    }

    #[test]
    fn copy_right_after_its_own_key_drops_that_recording() {
        // Fn down, C down a moment later: the recording was never meant.
        assert_eq!(decide(rec(0, false, true), false), Decision::DropRecordingThenCopy);
        assert_eq!(decide(rec(180, false, true), false), Decision::DropRecordingThenCopy);
        assert_eq!(decide(rec(2000, false, true), false), Decision::DropRecordingThenCopy);
    }

    #[test]
    fn copy_in_toggle_mode_drops_the_recording_its_key_began() {
        // Toggle mode marks a recording hands free from the first moment. The
        // Fn of the shortcut must still not leave one running.
        assert_eq!(decide(rec(120, true, true), false), Decision::DropRecordingThenCopy);
    }

    #[test]
    fn copy_deep_into_a_held_dictation_leaves_it_alone() {
        assert_eq!(decide(rec(2001, false, true), false), Decision::LeaveAlone);
        assert_eq!(decide(rec(20_000, false, true), false), Decision::LeaveAlone);
    }

    #[test]
    fn copy_never_touches_a_recording_on_other_keys() {
        // Dictation on Option and Space: the copy shortcut shares nothing.
        assert_eq!(decide(rec(50, false, false), false), Decision::Copy);
        assert_eq!(decide(rec(50, true, false), false), Decision::Copy);
        assert_eq!(decide(rec(60_000, true, false), false), Decision::Copy);
    }

    #[test]
    fn copy_while_text_is_on_its_way_waits_for_it() {
        assert_eq!(decide(None, true), Decision::CopyWhenTyped);
    }

    #[test]
    fn which_dictation_keys_the_copy_shortcut_sets_off() {
        assert!(sets_off("fn", "fn+c"));
        assert!(sets_off("Fn", "fn + C"));
        assert!(sets_off("function", "fn+c"));
        assert!(sets_off("option", "option+c"));
        assert!(sets_off("option_left", "alt+c"));
        assert!(sets_off("ctrl_right", "control+shift+c"));
        assert!(sets_off("ctrl+shift", "ctrl+shift+c"));
        // Other keys, or the very same shortcut, or nothing at all.
        assert!(!sets_off("option+space", "fn+c"));
        assert!(!sets_off("fn+space", "fn+c"));
        assert!(!sets_off("fn+c", "fn+c"));
        assert!(!sets_off("", "fn+c"));
        assert!(!sets_off("fn", ""));
    }

    #[test]
    fn a_waiting_copy_lapses() {
        let now = Instant::now();
        assert!(!still_wanted(None, now));
        assert!(still_wanted(Some(now), now));
        assert!(still_wanted(Some(now), now + PATIENCE));
        assert!(!still_wanted(Some(now), now + PATIENCE + ms(1)));
    }

    #[test]
    fn the_shortcut_is_fn_and_c() {
        let binding = binding();
        assert_eq!(binding.id, BINDING);
        assert_eq!(binding.current_binding, "fn+c");
        assert_eq!(binding.default_binding, binding.current_binding);
    }

    #[test]
    fn only_real_text_is_remembered() {
        remember("   ");
        // A blank must not replace what is there, whatever is there.
        let before = LAST_TYPED.lock().unwrap().clone();
        remember("");
        assert_eq!(*LAST_TYPED.lock().unwrap(), before);
        remember("Hello from the test.");
        assert_eq!(LAST_TYPED.lock().unwrap().as_deref(), Some("Hello from the test."));
    }
}
