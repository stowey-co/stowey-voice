# Stowey® Voice

Dictation for macOS, built for the Stowey team. Hold a key, speak, and clean text is typed into whatever app you are in.

Stowey® Voice is Stowey's build of [Handy](https://github.com/cjpais/Handy), the open source speech to text app by CJ Pais. Transcription and cleanup run on the Mac. Nothing is sent to a server.

## Download

Get the latest build from the [releases page](https://github.com/stowey-co/stowey-voice/releases/latest).

| Your Mac | Direct link |
|---|---|
| Apple silicon (M1 or newer) | https://github.com/stowey-co/stowey-voice/releases/latest/download/Stowey-Voice-apple-silicon.dmg |
| Intel | https://github.com/stowey-co/stowey-voice/releases/latest/download/Stowey-Voice-intel.dmg |

Not sure which Mac you have? Apple menu, About This Mac. If the chip line starts with Apple, take the first file.

## Install

1. Open the downloaded file and drag Stowey® Voice into Applications.
2. Open Stowey® Voice. macOS says it cannot verify the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll down and click Open Anyway next to Stowey® Voice. Confirm once.
4. Follow the setup inside the app. It asks for the microphone and for Accessibility, which it needs to type for you.

Steps 2 and 3 are needed once per download, because the build is not yet signed with an Apple Developer account. After an update, macOS asks for the Accessibility permission again for the same reason.

## What it does that stock Handy does not

1. **Cleanup is on from the first dictation.** The normal shortcut cleans the text. No second shortcut, no setup.
2. **It works while you talk.** The speech is decoded at every pause, during the dictation. When you let go of the key, only the words since your last pause are left to do.
3. **It only makes you wait where an editor earns it.** A cleanup model runs for an email, and for a text in which you corrected yourself, counted out a list or asked for a new paragraph. There, "at three, no wait, at four" becomes "at 4". Everything else is tidied by rule and typed at once: a throat clearing "So", an aside like "you know", a small word said twice.
4. **It knows where you are typing.** The app and window in focus decide the above. A chat with an AI is typed as spoken, because its reader copes. A terminal or code editor gets your words untouched.
5. **Stowey house style, enforced.** No dashes, no exclamation marks, British English. A deterministic gate runs last, so a stray dash never reaches the page. Document numbers, SKUs, dates, URLs and command flags keep their own characters. The gate stays out of terminals and code editors.
6. **It knows your words.** The speech model is told how Stowey is spelt, next to your custom words and your private word file. See below.
7. **It uses the Mac's own microphone.** It opens fastest. A Bluetooth headset needs a second or more to switch into call mode and then sends telephone quality sound, and a display's microphone takes about half a second to open each time. With the lid closed, or with a microphone chosen under Settings, yours is used instead.
8. **It tells you when it listens.** A low, soft sound plays once the microphone really is recording, and a lower one when you let go. Switch them off under Settings, General, Audio Feedback.
9. **The pill is glass.** Small, translucent, and only the bars. macOS draws the glass itself: Apple's own from macOS 26, a frosted one before. It wears the opposite of your Mac's mode, dark in light mode and light in dark mode, so it stands out. To change how see through it is without a new build, put a file named `pill.txt` beside the app's log, in `~/Library/Logs/co.stowey.voice/`, with lines such as `glass = clear` or `glass = regular`, and `tint = 0.2`.
10. **It never updates itself into stock Handy.** New versions come from this repository only.
11. **It takes over from an older copy.** If an earlier build is still running under the old name Stowey Voice, the new one asks it to quit. Otherwise the old one would keep answering the shortcut.

The cleanup model is Apple Intelligence, on the Mac itself. That needs an Apple silicon Mac on macOS 26 or newer with Apple Intelligence switched on. On other Macs everything else still works. Any provider Handy supports can be selected instead under Settings, Post Process.

Switching Post Process off in Settings switches off the model, the tidy rules and the gate.

## Your private word file

Names of colleagues, customers and suppliers are not in this public code. They go into a plain text file on your Mac:

`~/Library/Logs/co.stowey.voice/vocabulary.txt`

One name or term per line. A line `said => written` is a replacement: say the left side, and the right side is typed. Lines starting with `#` are notes. The app reads the file again whenever it changes. The same file is also read from `~/Library/Application Support/co.stowey.voice/`.

Ask your Claude to build it from the names you use most. It can write to the first folder once you share it.

## How this repository works

The fork is a thin overlay. Upstream's sources stay untouched in git. Everything Stowey adds lives in nine places:

| Path | What it is |
|---|---|
| `stowey/stowey.rs` | The native module: defaults, destinations, cleanup policy, tidy rules, prompt, vocabulary, microphone choice, house style gate, with tests |
| `stowey/stowey_live.rs` | Cuts a live recording at every pause so it is decoded while you talk, and joins the pieces, with tests |
| `stowey/pill.css` | The parts of the recording pill: bars, spinner, sizes. Not the glass |
| `stowey/stowey_mac.rs` | What only macOS can do: the glass behind the pill, and asking an older copy of the app to quit |
| `stowey/stowey_glass.rs` | Lets the pill tell the app where it is, so the glass can follow it |
| `stowey/stoweyGlass.ts` | The script in the pill's web view that does the telling |
| `stowey/apply.py` | Applies the overlay to a clean checkout: wires the modules in, adds the warm up call to the Apple Intelligence bridge, draws the two sounds and the icons, renames the app, sets the palette and typeface |
| `.github/workflows/stowey-voice.yml` | Builds both apps on GitHub's Mac build machines, tests the module and publishes a release |
| `README.md` | This file |

Every patch in `apply.py` asserts that its anchor still exists. If an upstream change breaks one, the build fails loudly instead of silently shipping stock behaviour.

### Build a new version

Push a change under `stowey/`, or run the Stowey® Voice workflow by hand from the Actions tab. The build takes up to an hour. The new release replaces the latest one only when both installers are attached and the tests pass, so the download links above never break.

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
