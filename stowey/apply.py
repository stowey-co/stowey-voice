#!/usr/bin/env python3
"""Turns a clean checkout of Handy into Stowey Voice.

Run from the repository root, before the build. Needs Python with Pillow,
and bun on the PATH:

    python stowey/apply.py

The fork keeps upstream's sources untouched in git and applies Stowey's
changes here, at build time. That keeps the fork a thin overlay: pulling a new
upstream release rarely conflicts, and every change Stowey makes is listed in
this one file. Each patch asserts that its anchor still exists, so an upstream
change that breaks a patch fails loudly here instead of silently shipping
stock behaviour.
"""

from __future__ import annotations

import json
import math
import shutil
import struct
import subprocess
import sys
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OVERLAY = ROOT / "stowey"
TAURI = ROOT / "src-tauri"
SRC = ROOT / "src"

NAME = "Stowey\u00ae Voice"
# The same without the mark, for the few places that only take plain ASCII.
PLAIN_NAME = "Stowey Voice"
IDENTIFIER = "co.stowey.voice"
REPO = "stowey-co/stowey-voice"

INK = "#0d0d0d"
GROUND = "#e9e9e9"
ALARM = "#c0392b"


def patch(path: Path, old: str, new: str, count: int = 1) -> None:
    text = path.read_text(encoding="utf-8")
    found = text.count(old)
    if found != count:
        sys.exit(
            f"stowey/apply.py: expected {count} match(es) in {path.relative_to(ROOT)}, "
            f"found {found}:\n{old}"
        )
    path.write_text(text.replace(old, new), encoding="utf-8")


# ---------------------------------------------------------------------------
# Rust: behaviour
# ---------------------------------------------------------------------------


def patch_rust() -> None:
    src = TAURI / "src"
    shutil.copyfile(OVERLAY / "stowey.rs", src / "stowey.rs")
    shutil.copyfile(OVERLAY / "stowey_live.rs", src / "stowey_live.rs")

    lib = src / "lib.rs"
    patch(lib, "mod signal_handle;\n", "mod signal_handle;\nmod stowey;\nmod stowey_live;\n")
    patch(lib, '.title("Handy")', f'.title("{NAME}")')
    # Load the speech model at launch and start watching the microphones.
    patch(
        lib,
        "    app_handle.manage(TranscriptionJobManager::new(app_handle));\n",
        "    app_handle.manage(TranscriptionJobManager::new(app_handle));\n"
        "    crate::stowey::warm_start(app_handle);\n",
    )

    actions = src / "actions.rs"
    # 1. The normal dictation shortcut cleans up by default.
    patch(
        actions,
        "        let post_process = self.post_process;\n",
        "        let post_process =\n"
        "            self.post_process || crate::stowey::cleanup_by_default(&get_settings(app));\n",
    )
    # 2. The cleanup model is told which app and window the text goes into.
    patch(
        actions,
        "    if prompt.trim().is_empty() {\n"
        '        debug!("Post-processing skipped because the selected prompt is empty");',
        "    let prompt = crate::stowey::fill_prompt(&prompt, settings);\n"
        "    if prompt.trim().is_empty() {\n"
        '        debug!("Post-processing skipped because the selected prompt is empty");',
    )
    # 3. The house style gate runs last, right before the text is typed.
    patch(
        actions,
        "crate::transcription_hook::apply(&ah, processed.final_text),",
        "crate::transcription_hook::apply(\n"
        "                                    &ah,\n"
        "                                    crate::stowey::house_style(\n"
        "                                        &get_settings(&ah),\n"
        "                                        processed.final_text,\n"
        "                                    ),\n"
        "                                ),",
    )

    # 4. The moment a dictation starts, note where the text is going and get
    #    the cleanup model ready while the user is still speaking.
    patch(
        actions,
        "    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {\n",
        "    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {\n"
        "        crate::stowey::prewarm(app.clone());\n",
    )
    # 5. A short dictation with nothing to clean goes straight through. The
    #    model would only add a wait.
    patch(
        actions,
        '        debug!("Post-processing skipped because the transcription is empty");\n'
        "        return PostProcessOutcome::Skipped;\n"
        "    }\n",
        '        debug!("Post-processing skipped because the transcription is empty");\n'
        "        return PostProcessOutcome::Skipped;\n"
        "    }\n"
        "    if crate::stowey::skip_cleanup(transcription) {\n"
        '        debug!("Post-processing skipped: short and clean");\n'
        "        return PostProcessOutcome::Skipped;\n"
        "    }\n",
    )

    # 6. With no microphone chosen in Settings, a Bluetooth headset yields to
    #    the Mac's own microphone. The clamshell variant is the one that falls
    #    back to the system default without touching the stored choice.
    patch(
        src / "managers" / "audio.rs",
        "            None => DesiredMicrophone::Default,\n",
        "            None => match crate::stowey::preferred_microphone() {\n"
        "                Some(name) => DesiredMicrophone::Clamshell(name),\n"
        "                None => DesiredMicrophone::Default,\n"
        "            },\n",
    )
    # 7. Stowey's names go to the speech model with the user's custom words.
    patch(
        src / "managers" / "transcription.rs",
        "    if supports_vocabulary {\n        custom_words.to_vec()\n",
        "    if supports_vocabulary {\n        crate::stowey::with_house_vocabulary(custom_words)\n",
    )

    # 8. A live recording is cut at every pause and decoded while the user is
    #    still speaking. A whole clip keeps upstream's chunker.
    chunked = src / "managers" / "transcription" / "chunked.rs"
    patch(
        chunked,
        "use crate::audio_toolkit::chunking::Chunker;\n",
        "use crate::stowey_live::Chunker;\n",
    )
    patch(
        chunked,
        "        let mut decode = Self::new(tm, mark, setup, model_id, backend);\n"
        "        decode.push(audio);\n",
        "        let mut decode = Self::new(tm, mark, setup, model_id, backend);\n"
        "        decode.chunker.whole_clip();\n"
        "        decode.push(audio);\n",
    )
    # 9. The chunks' texts are joined with their seams mended.
    transcription = src / "managers" / "transcription.rs"
    patch(transcription, "use crate::audio_toolkit::chunking;\n", "")
    patch(
        transcription,
        "            chunking::join_transcripts(&many.iter().map(|t| t.text.as_str()).collect::<Vec<_>>())\n",
        "            crate::stowey_live::join(&many.iter().map(|t| t.text.as_str()).collect::<Vec<_>>())\n",
    )

    settings = src / "settings.rs"
    # Stowey's prompt and speed settings, applied once per install.
    patch(
        settings,
        "    if ensure_post_process_defaults(&mut settings) {\n"
        '        store.set("settings", serde_json::to_value(&settings).unwrap());\n'
        "    }\n",
        "    if ensure_post_process_defaults(&mut settings) {\n"
        '        store.set("settings", serde_json::to_value(&settings).unwrap());\n'
        "    }\n"
        "    if crate::stowey::migrate(app, &mut settings) {\n"
        '        store.set("settings", serde_json::to_value(&settings).unwrap());\n'
        "    }\n",
    )
    patch(
        settings,
        "fn default_post_process_enabled() -> bool {\n    false\n}",
        "fn default_post_process_enabled() -> bool {\n    true\n}",
    )
    patch(
        settings,
        'fn default_post_process_provider_id() -> String {\n    "openai".to_string()\n}',
        "fn default_post_process_provider_id() -> String {\n"
        "    crate::stowey::default_provider_id()\n}",
    )
    patch(
        settings,
        "fn default_post_process_prompts() -> Vec<LLMPrompt> {\n    vec![LLMPrompt {",
        "fn default_post_process_prompts() -> Vec<LLMPrompt> {\n"
        "    if true {\n"
        "        return crate::stowey::default_prompts();\n"
        "    }\n"
        "    vec![LLMPrompt {",
    )
    patch(
        settings,
        "    #[serde(default)]\n    pub post_process_selected_prompt_id: Option<String>,",
        '    #[serde(default = "crate::stowey::default_selected_prompt_id")]\n'
        "    pub post_process_selected_prompt_id: Option<String>,",
    )
    patch(
        settings,
        "        post_process_selected_prompt_id: None,",
        "        post_process_selected_prompt_id: crate::stowey::default_selected_prompt_id(),",
    )
    # Stowey's app UI system is a light ground. Dark stays available in settings.
    patch(
        settings,
        "fn default_theme() -> Theme {\n    Theme::System\n}",
        "fn default_theme() -> Theme {\n    Theme::Light\n}",
    )
    # Never self update: upstream's updater would replace this app with stock
    # Handy. New versions come from this repository's releases.
    patch(
        settings,
        '.get_or_init(|| utils::env_flag_enabled("HANDY_DISABLE_UPDATER") || !self_update_possible())',
        ".get_or_init(|| {\n"
        '            true || utils::env_flag_enabled("HANDY_DISABLE_UPDATER") || !self_update_possible()\n'
        "        })",
    )

    tray = src / "tray.rs"
    patch(tray, 'format!("Handy v{} (Dev)"', f'format!("{NAME} v{{}} (Dev)"')
    patch(tray, 'format!("Handy v{}"', f'format!("{NAME} v{{}}"')

    patch(
        src / "cli.rs",
        'about = "Handy - Speech to Text"',
        f'about = "{NAME}, speech to text"',
    )

    patch(
        src / "llm_client.rs",
        'headers.insert("X-Title", HeaderValue::from_static("Handy"));',
        f'headers.insert("X-Title", HeaderValue::from_static("{PLAIN_NAME}"));',
    )


def patch_build_script() -> None:
    """Lets an Intel Mac build the Apple silicon app, cleanup model included.

    Upstream only builds the Apple Intelligence bridge when the build machine
    itself is Apple silicon. GitHub's Apple silicon build machines are often
    out of capacity while its Intel ones are free, so the bridge is built
    whenever the target is Apple silicon, whatever the machine.
    """
    build = TAURI / "build.rs"
    gate = '#[cfg(all(target_os = "macos", target_arch = "aarch64"))]'
    patch(
        build,
        f"    {gate}\n    build_apple_intelligence_bridge();\n",
        '    #[cfg(target_os = "macos")]\n'
        "    {\n"
        '        if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {\n'
        "            build_apple_intelligence_bridge();\n"
        "        }\n"
        "    }\n",
    )
    patch(build, gate, '#[cfg(target_os = "macos")]', count=2)


SWIFT_WARM_SESSION = '''

// ---------------------------------------------------------------------------
// Stowey Voice: get a session ready while the user is still speaking.
//
// Loading the model and reading the instructions is most of the wait after a
// dictation. `stowey_prewarm_apple_intelligence` does both the moment the
// recording starts; the cleanup call above then picks the warm session up,
// provided its instructions are word for word the same.
// ---------------------------------------------------------------------------

@available(macOS 26.0, *)
private final class StoweyWarmSession: @unchecked Sendable {
    static let shared = StoweyWarmSession()
    private let lock = NSLock()
    private var session: LanguageModelSession?
    private var instructions: String?

    func warm(_ newInstructions: String) {
        lock.lock()
        let alreadyWarm = session != nil && instructions == newInstructions
        lock.unlock()
        if alreadyWarm { return }

        let model = SystemLanguageModel.default
        guard model.availability == .available else { return }
        let fresh = LanguageModelSession(model: model, instructions: newInstructions)
        fresh.prewarm()
        lock.lock()
        session = fresh
        instructions = newInstructions
        lock.unlock()
    }

    func take(_ wanted: String) -> LanguageModelSession? {
        lock.lock()
        defer { lock.unlock() }
        guard instructions == wanted, let ready = session else { return nil }
        session = nil
        instructions = nil
        return ready
    }
}

@_cdecl("stowey_prewarm_apple_intelligence")
public func stoweyPrewarmAppleIntelligence(_ systemPrompt: UnsafePointer<CChar>) {
    guard #available(macOS 26.0, *) else { return }
    StoweyWarmSession.shared.warm(String(cString: systemPrompt))
}
'''

SWIFT_WARM_STUB = '''

// Stowey Voice: nothing to warm up in a build without Apple Intelligence.
@_cdecl("stowey_prewarm_apple_intelligence")
public func stoweyPrewarmAppleIntelligence(_ systemPrompt: UnsafePointer<CChar>) {
}
'''


def patch_swift() -> None:
    """Adds session warm up to the Apple Intelligence bridge, real and stub."""
    real = TAURI / "swift" / "apple_intelligence.swift"
    patch(
        real,
        "            let session = LanguageModelSession(\n"
        "                model: model,\n"
        "                instructions: swiftSystemPrompt\n"
        "            )\n",
        "            let session = StoweyWarmSession.shared.take(swiftSystemPrompt)\n"
        "                ?? LanguageModelSession(\n"
        "                    model: model,\n"
        "                    instructions: swiftSystemPrompt\n"
        "                )\n",
    )
    real.write_text(real.read_text(encoding="utf-8") + SWIFT_WARM_SESSION, encoding="utf-8")

    stub = TAURI / "swift" / "apple_intelligence_stub.swift"
    # Fails loudly if upstream renames the bridge, like every other patch here.
    patch(
        stub,
        '@_cdecl("process_text_with_system_prompt_apple")',
        '@_cdecl("process_text_with_system_prompt_apple")',
    )
    stub.write_text(stub.read_text(encoding="utf-8") + SWIFT_WARM_STUB, encoding="utf-8")


# ---------------------------------------------------------------------------
# Sounds
# ---------------------------------------------------------------------------

SOUND_RATE = 48_000


def bloop(path: Path, start_hz: float, end_hz: float, seconds: float, level: float) -> None:
    """A soft tone that glides from one pitch to another and fades: up for
    "listening", down for "done". One partial and a quiet octave above it, so
    it stays a tone and never sounds like a voice to the microphone."""
    frames = int(SOUND_RATE * seconds)
    glide = 0.45 * seconds
    attack = 0.006
    release = 0.03
    phase = 0.0
    samples = []
    for n in range(frames):
        t = n / SOUND_RATE
        # The pitch moves fast at first and settles, like a drop into water.
        progress = 1.0 - math.exp(-4.0 * min(t / glide, 4.0))
        hz = start_hz + (end_hz - start_hz) * progress
        phase += 2.0 * math.pi * hz / SOUND_RATE
        body = math.sin(phase) + 0.16 * math.exp(-t / 0.03) * math.sin(2.0 * phase)
        rise = 0.5 - 0.5 * math.cos(math.pi * min(t / attack, 1.0))
        decay = math.exp(-t / (0.42 * seconds))
        fall = 0.5 - 0.5 * math.cos(math.pi * min((seconds - t) / release, 1.0))
        samples.append(level * body * rise * decay * fall)
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(SOUND_RATE)
        out.writeframes(
            b"".join(struct.pack("<hh", *(2 * [int(max(-1.0, min(1.0, s)) * 32767)])) for s in samples)
        )


def patch_sounds() -> None:
    """Stowey's own two sounds take the place of upstream's "Pop" theme, which
    `stowey.rs` switches on. Drawn here, so no sound file is copied from anyone."""
    resources = TAURI / "resources"
    for name in ("pop_start.wav", "pop_stop.wav"):
        if not (resources / name).exists():
            sys.exit(f"stowey/apply.py: expected {name} in src-tauri/resources")
    bloop(resources / "pop_start.wav", 520.0, 880.0, 0.17, 0.30)
    bloop(resources / "pop_stop.wav", 740.0, 440.0, 0.19, 0.26)
    patch(
        SRC / "components" / "settings" / "SoundPicker.tsx",
        '{ value: "pop", label: "Pop" },',
        '{ value: "pop", label: "Stowey" },',
    )


# ---------------------------------------------------------------------------
# The pill
# ---------------------------------------------------------------------------


def patch_pill() -> None:
    """The recording pill as glass: `stowey/pill.css` goes on the end of the
    overlay's stylesheet, and the window gets room for the pill's shadow."""
    css = SRC / "overlay" / "RecordingOverlay.css"
    text = css.read_text(encoding="utf-8")
    for needed in (".scard {", ".scard.compact {", ".ov-stage {", ".swave i {", ".sx {"):
        if needed not in text:
            sys.exit(f"stowey/apply.py: the overlay stylesheet no longer has {needed}")
    pill = (OVERLAY / "pill.css").read_text(encoding="utf-8")
    css.write_text(text.rstrip("\n") + "\n\n" + pill, encoding="utf-8")

    overlay = TAURI / "src" / "overlay.rs"
    # 22 more points of height: 14 under the pill for its shadow, 8 above.
    patch(overlay, "const OVERLAY_HEIGHT: f64 = 50.0;", "const OVERLAY_HEIGHT: f64 = 72.0;")
    # And 14 lower, so the pill itself stays 15 above the edge of the screen.
    patch(
        overlay,
        '#[cfg(target_os = "macos")]\nconst OVERLAY_BOTTOM_OFFSET: f64 = 15.0;',
        '#[cfg(target_os = "macos")]\nconst OVERLAY_BOTTOM_OFFSET: f64 = 1.0;',
    )


# ---------------------------------------------------------------------------
# App identity
# ---------------------------------------------------------------------------


def patch_identity() -> None:
    conf_path = TAURI / "tauri.conf.json"
    conf = json.loads(conf_path.read_text(encoding="utf-8"))
    conf["productName"] = NAME
    conf["identifier"] = IDENTIFIER
    conf["bundle"]["createUpdaterArtifacts"] = False
    conf["plugins"]["updater"]["endpoints"] = [
        f"https://github.com/{REPO}/releases/latest/download/latest.json"
    ]
    conf_path.write_text(json.dumps(conf, indent=2) + "\n", encoding="utf-8")

    patch(ROOT / "index.html", "<title>handy</title>", f"<title>{NAME}</title>")


def patch_strings() -> None:
    locales = SRC / "i18n" / "locales"
    for path in sorted(locales.glob("*/translation.json")):
        text = path.read_text(encoding="utf-8")
        path.write_text(text.replace("Handy", NAME), encoding="utf-8")
        json.loads(path.read_text(encoding="utf-8"))

    # The donate button supports Handy's author, so it says so.
    patch(
        locales / "en" / "translation.json",
        f'"description": "Help us continue building {NAME}"',
        f'"description": "{NAME} is built on Handy, an open source project. Support its author."',
    )


# ---------------------------------------------------------------------------
# Look: monochrome palette, Archivo, Stowey marks
# ---------------------------------------------------------------------------

# One glyph, shared by the app icon, the menu bar icons and the in app marks:
# five bars of a voice level meter. Geometry on a 64 unit grid.
BARS = [(9.0, 14.0), (19.0, 28.0), (29.0, 42.0), (39.0, 28.0), (49.0, 14.0)]
BAR_WIDTH = 6.0
SMALL_BARS = [(16.0, 10.0), (23.0, 20.0), (30.0, 30.0), (37.0, 20.0), (44.0, 10.0)]
SMALL_BAR_WIDTH = 4.0
DOTS = [20.0, 32.0, 44.0]

TRAY_ICON_TSX = '''/** The menu bar icon. Same geometry as src-tauri/resources/tray_*.png. */
type TrayIconState = "idle" | "recording" | "transcribing";

const PATHS: Record<TrayIconState, string> = {
  idle: "@@IDLE@@",
  recording: "@@RECORDING@@",
  transcribing: "@@TRANSCRIBING@@",
};

const TrayIcon = ({
  state,
  size,
}: {
  state: TrayIconState;
  size: number | string;
}) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 64 64"
    className="fill-current"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path fillRule="evenodd" d={PATHS[state]} />
  </svg>
);

export default TrayIcon;
'''

MARK_TSX = '''/** The Stowey Voice mark: five bars of a voice level meter. */
const HandyHand = ({
  width,
  height,
}: {
  width?: number | string;
  height?: number | string;
  // The sidebar passes a className for its icons; the mark ignores it.
  className?: string;
}) => (
  <svg
    width={width || 126}
    height={height || 135}
    viewBox="0 0 64 64"
    className="fill-text"
    xmlns="http://www.w3.org/2000/svg"
  >
    <path d="@@IDLE@@" />
  </svg>
);

export default HandyHand;
'''

# The onboarding mascot: the mark on a rounded tile. 151 by 165 keeps
# upstream's box so the layout and the arrival animation still fit.
MASCOT_TSX = '''/** The Stowey Voice mark on a tile; follows the theme. */
const HandyMascot = ({ className = "" }: { className?: string }) => {
  return (
    <svg
      viewBox="0 0 151 165"
      className={className}
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path className="fill-text" d="@@TILE@@" />
      <path className="fill-background" d="@@BARS@@" />
    </svg>
  );
};

export default HandyMascot;
'''

WORDMARK_TSX = '''/** The wordmark, set in the interface typeface. */
const HandyTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 720 130"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="@@NAME@@"
    >
      <text
        x="0"
        y="100"
        className="fill-text"
        fontSize="104"
        fontWeight="800"
        textLength="720"
        lengthAdjust="spacingAndGlyphs"
      >
        Stowey
        <tspan fontSize="52" dy="-40">
          ®
        </tspan>
        <tspan dy="40"> Voice</tspan>
      </text>
    </svg>
  );
};

export default HandyTextLogo;
'''


def pill_path(x: float, y: float, w: float, h: float) -> str:
    r = w / 2
    return (
        f"M{x + r:g} {y:g}a{r:g} {r:g} 0 0 1 {r:g} {r:g}v{h - 2 * r:g}"
        f"a{r:g} {r:g} 0 0 1 {-w:g} 0v{-(h - 2 * r):g}a{r:g} {r:g} 0 0 1 {r:g} {-r:g}Z"
    )


def circle_path(cx: float, cy: float, r: float) -> str:
    return f"M{cx - r:g} {cy:g}a{r:g} {r:g} 0 1 0 {2 * r:g} 0a{r:g} {r:g} 0 1 0 {-2 * r:g} 0Z"


def bars_path(bars, width: float, scale: float = 1.0, dx: float = 0.0, dy: float = 0.0) -> str:
    parts = []
    for x, h in bars:
        parts.append(
            pill_path(dx + x * scale, dy + (32 - h / 2) * scale, width * scale, h * scale)
        )
    return "".join(parts)


def tray_paths() -> dict[str, str]:
    disc = circle_path(32, 32, 27)
    return {
        "idle": bars_path(BARS, BAR_WIDTH),
        "recording": disc + bars_path(SMALL_BARS, SMALL_BAR_WIDTH),
        "transcribing": disc + "".join(circle_path(x, 32, 4) for x in DOTS),
    }


def write_components() -> None:
    icons = SRC / "components" / "icons"
    paths = tray_paths()
    tile = (
        "M38 14h75a24 24 0 0 1 24 24v89a24 24 0 0 1 -24 24h-75a24 24 0 0 1 -24 -24v-89"
        "a24 24 0 0 1 24 -24Z"
    )
    files = {
        "TrayIcon.tsx": TRAY_ICON_TSX.replace("@@IDLE@@", paths["idle"])
        .replace("@@RECORDING@@", paths["recording"])
        .replace("@@TRANSCRIBING@@", paths["transcribing"]),
        "HandyHand.tsx": MARK_TSX.replace("@@IDLE@@", paths["idle"]),
        "HandyMascot.tsx": MASCOT_TSX.replace("@@TILE@@", tile).replace(
            "@@BARS@@", bars_path(BARS, BAR_WIDTH, scale=1.5, dx=27.5, dy=34.5)
        ),
        "HandyTextLogo.tsx": WORDMARK_TSX.replace("@@NAME@@", NAME),
    }
    for name, content in files.items():
        if not (icons / name).exists():
            sys.exit(f"stowey/apply.py: upstream no longer ships src/components/icons/{name}")
        (icons / name).write_text(content, encoding="utf-8")


def patch_theme() -> None:
    theme = SRC / "styles" / "theme.css"
    swaps = [
        ("--light-color-text: #0f0f0f;", f"--light-color-text: {INK};"),
        ("--light-color-background: #fbfbfb;", f"--light-color-background: {GROUND};"),
        ("--light-color-logo-primary: #faa2ca;", f"--light-color-logo-primary: {INK};"),
        ("--light-color-logo-stroke: #382731;", f"--light-color-logo-stroke: {INK};"),
        ("--dark-color-logo-primary: #f28cbb;", "--dark-color-logo-primary: #f2f2f2;"),
        ("--light-color-warning: #d97706;", f"--light-color-warning: {ALARM};"),
        ("--light-color-error: #dc2626;", f"--light-color-error: {ALARM};"),
        ("--light-color-moss: #2f7d5b;", f"--light-color-moss: {INK};"),
        ("--dark-color-moss: #86c9a4;", "--dark-color-moss: #f2f2f2;"),
        ("--color-background-ui: #da5893;", f"--color-background-ui: {INK};"),
        ("color-mix(in srgb, #c7b9ef 55%, #fbfbfb) 0%,", "color-mix(in srgb, #cfcfcf 55%, #fbfbfb) 0%,"),
        ("color-mix(in srgb, #e9e1f3 60%, #fbfbfb);", "color-mix(in srgb, #dedede 60%, #fbfbfb);"),
        ("color-mix(in srgb, #7c68c0 38%, #1b1719) 0%,", "color-mix(in srgb, #6e6e6e 38%, #1a1a1a) 0%,"),
    ]
    for old, new in swaps:
        patch(theme, old, new)

    # Archivo, bundled with the app so nothing loads from the network.
    subprocess.run(["bun", "add", "@fontsource-variable/archivo@^5.2.5"], cwd=ROOT, check=True)

    app_css = SRC / "App.css"
    patch(
        app_css,
        '@import "./styles/theme.css";\n',
        '@import "./styles/theme.css";\n@import "@fontsource-variable/archivo/index.css";\n',
    )
    patch(
        app_css,
        "  --font-display:\n    ui-rounded,",
        '  --font-display:\n    "Archivo Variable", ui-rounded,',
    )
    patch(
        app_css,
        "  --font-ui:\n    system-ui,",
        '  --font-ui:\n    "Archivo Variable", system-ui,',
    )


def draw_brand_images() -> None:
    """App icon source and menu bar icons, drawn from the shared geometry."""
    from PIL import Image, ImageDraw

    def bars(draw, items, width, scale, dx, dy, fill):
        for x, h in items:
            left = dx + x * scale
            top = dy + (32 - h / 2) * scale
            draw.rounded_rectangle(
                [left, top, left + width * scale, top + h * scale],
                radius=width * scale / 2,
                fill=fill,
            )

    def tray(state: str, colour, warning: bool = False, badge=None):
        ss = 8  # supersampling
        size = 64 * ss
        mask = Image.new("L", (size, size), 0)
        draw = ImageDraw.Draw(mask)
        if state == "idle":
            bars(draw, BARS, BAR_WIDTH, ss, 0, 0, 255)
        else:
            draw.ellipse([5 * ss, 5 * ss, 59 * ss, 59 * ss], fill=255)
            if state == "recording":
                bars(draw, SMALL_BARS, SMALL_BAR_WIDTH, ss, 0, 0, 0)
            else:
                for x in DOTS:
                    draw.ellipse([(x - 4) * ss, 28 * ss, (x + 4) * ss, 36 * ss], fill=0)
        image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        image.paste(Image.new("RGBA", (size, size), colour), (0, 0), mask)
        if warning:
            draw_rgba = ImageDraw.Draw(image)
            draw_rgba.ellipse([32 * ss, 32 * ss, 64 * ss, 64 * ss], fill=(0, 0, 0, 0))
            badge_mask = Image.new("L", (size, size), 0)
            badge_draw = ImageDraw.Draw(badge_mask)
            badge_draw.ellipse([35 * ss, 35 * ss, 61 * ss, 61 * ss], fill=255)
            badge_draw.rounded_rectangle(
                [46.5 * ss, 40 * ss, 49.5 * ss, 51 * ss], radius=1.5 * ss, fill=0
            )
            badge_draw.ellipse([46.2 * ss, 53 * ss, 49.8 * ss, 56.6 * ss], fill=0)
            image.paste(
                Image.new("RGBA", (size, size), badge or colour), (0, 0), badge_mask
            )
        return image.resize((64, 64), Image.LANCZOS)

    white = (255, 255, 255, 255)
    black = (13, 13, 13, 255)
    alarm = (192, 57, 43, 255)
    resources = TAURI / "resources"
    outputs = {
        "tray_idle.png": tray("idle", white),
        "tray_idle_dark.png": tray("idle", black),
        "tray_idle_warning.png": tray("idle", white, warning=True),
        "tray_idle_warning_dark.png": tray("idle", black, warning=True),
        "tray_recording.png": tray("recording", white),
        "tray_recording_dark.png": tray("recording", black),
        "tray_transcribing.png": tray("transcribing", white),
        "tray_transcribing_dark.png": tray("transcribing", black),
        "handy.png": tray("idle", white),
        "handy_warning.png": tray("idle", white, warning=True, badge=alarm),
        "recording.png": tray("recording", white),
        "transcribing.png": tray("transcribing", white),
    }
    for name, image in outputs.items():
        if not (resources / name).exists():
            sys.exit(f"stowey/apply.py: upstream no longer ships resources/{name}")
        image.save(resources / name)

    # App icon: Apple's grid, an 824 px tile on a 1024 px canvas.
    build = OVERLAY / "build"
    build.mkdir(exist_ok=True)
    custom = OVERLAY / "brand" / "app-icon.png"
    target = build / "app-icon.png"
    if custom.exists():
        shutil.copyfile(custom, target)
    else:
        ss = 4
        size = 1024 * ss
        icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        draw = ImageDraw.Draw(icon)
        draw.rounded_rectangle(
            [100 * ss, 100 * ss, 924 * ss, 924 * ss], radius=185 * ss, fill=black
        )
        scale = 9.5 * ss
        offset = (1024 * ss - 64 * scale) / 2
        bars(draw, BARS, BAR_WIDTH, scale, offset, offset, white)
        icon.resize((1024, 1024), Image.LANCZOS).save(target)
    shutil.copyfile(target, TAURI / "icons" / "logo.png")


def main() -> None:
    marker = OVERLAY / "build" / ".applied"
    if marker.exists():
        sys.exit("stowey/apply.py: already applied. Run it on a clean checkout.")
    patch_rust()
    patch_swift()
    patch_build_script()
    patch_identity()
    patch_strings()
    write_components()
    patch_theme()
    patch_sounds()
    patch_pill()
    draw_brand_images()
    marker.write_text("applied\n", encoding="utf-8")
    print(f"{PLAIN_NAME}: overlay applied.")


if __name__ == "__main__":
    main()
