# Stowey® Voice

Dictation for macOS, built for the Stowey team. Hold a key, speak, and clean text is typed into whatever app you are in.

Stowey Voice is Stowey's build of [Handy](https://github.com/cjpais/Handy), the open source speech to text app by CJ Pais. Transcription runs on the Mac. Nothing is sent to a server.

## Download

Get the latest build from the [releases page](https://github.com/RaphaelChemaly/stowey-voice/releases/latest).

| Your Mac | Direct link |
|---|---|
| Apple silicon (M1 or newer) | https://github.com/RaphaelChemaly/stowey-voice/releases/latest/download/Stowey-Voice-apple-silicon.dmg |
| Intel | https://github.com/RaphaelChemaly/stowey-voice/releases/latest/download/Stowey-Voice-intel.dmg |

Not sure which Mac you have? Apple menu, About This Mac. If the chip line starts with Apple, take the first file.

## Install

1. Open the downloaded file and drag Stowey Voice into Applications.
2. Open Stowey Voice. macOS says it cannot verify the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll down and click Open Anyway next to Stowey Voice. Confirm once.
4. Follow the setup inside the app. It asks for the microphone and for Accessibility, which it needs to type for you.

Steps 2 and 3 are needed once per download, because the build is not yet signed with an Apple Developer account. After an update, macOS asks for the Accessibility permission again for the same reason.

## What it does that stock Handy does not

1. **Cleanup is on from the first dictation.** The normal shortcut cleans the text. No second shortcut, no setup.
2. **It knows where you are typing.** The app and window in focus are handed to the cleanup model, so a chat message reads like a chat message and an email reads like an email.
3. **It removes what you did not mean to write.** Filler words, repeats and self corrections go. "At three, no wait, at four" becomes "at 4".
4. **Stowey house style, enforced.** No dashes, no exclamation marks, British English. A deterministic gate runs after the model, so a stray dash never reaches the page. Document numbers, SKUs, dates, URLs and command flags keep their own characters. The gate stays out of terminals and code editors.
5. **It never updates itself into stock Handy.** New versions come from this repository only.

Cleanup uses Apple Intelligence, on the Mac itself. That needs an Apple silicon Mac on macOS 26 or newer with Apple Intelligence switched on. On other Macs dictation still works and the house style gate still runs, without the model. Any provider Handy supports can be selected instead under Settings, Post Processing.

Switching Post Processing off in Settings switches off both the model and the gate.

## How this repository works

The fork is a thin overlay. Upstream's sources stay untouched in git. Everything Stowey adds lives in four places:

| Path | What it is |
|---|---|
| `stowey/stowey.rs` | The native module: defaults, context, prompt, house style gate, with tests |
| `stowey/apply.py` | Applies the overlay to a clean checkout: wires the module in, renames the app, draws the icons, sets the palette and typeface |
| `.github/workflows/stowey-voice.yml` | Builds both apps on GitHub's Mac build machines, tests the module and publishes a release |
| `README.md` | This file |

Every patch in `apply.py` asserts that its anchor still exists. If an upstream change breaks one, the build fails loudly instead of silently shipping stock behaviour.

### Build a new version

Push a change under `stowey/`, or run the Stowey Voice workflow by hand from the Actions tab. The build takes up to an hour. The new release replaces the latest one only when both installers are attached and the tests pass, so the download links above never break.

### Pull in a new Handy release

Sync the fork with upstream on GitHub (Sync fork, Update branch), then run the workflow. Only `README.md` can conflict.

### Build locally

On a Mac with Xcode, Rust, bun and Python with Pillow:

```bash
bun install
python3 stowey/apply.py
bun run tauri icon stowey/build/app-icon.png
bun run tauri build
```

Run `apply.py` on a clean checkout only. It changes upstream files in place, so do not commit its output.

### Change the icon

Put a 1024 by 1024 PNG at `stowey/brand/app-icon.png` and push. The build uses it instead of the drawn one.

## Credits and licence

Built on [Handy](https://github.com/cjpais/Handy) by CJ Pais and contributors, MIT licensed. See [LICENSE](LICENSE). The Handy name, logo and brand assets belong to the Handy project and are not used here.

Stowey® is a registered trade mark of Stowey UG (haftungsbeschränkt).
