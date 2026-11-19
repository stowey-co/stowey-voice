Stowey® Voice for macOS. Hold a key, speak, and clean text is typed into whatever app you are in.

## Which file

| Your Mac | File |
|---|---|
| Apple silicon (M1 or newer) | `Stowey-Voice-apple-silicon.dmg` |
| Intel | `Stowey-Voice-intel.dmg` |

## Install

1. Open the downloaded file and drag Stowey Voice into Applications.
2. Open Stowey Voice. macOS says it cannot verify the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll down and click Open Anyway next to Stowey Voice. Confirm once.
4. Follow the setup inside the app. It asks for the microphone and for Accessibility, which it needs to type for you.

Step 2 and 3 are needed once per download. They exist because this build is not yet signed with an Apple Developer account.

## New in this build

1. **It works while you talk.** The speech is decoded at every pause, during the dictation. When you let go of the key, only the words since your last pause are left to do.
2. **The cleanup model only runs where it earns its wait.** For an email, and for a text in which you corrected yourself or dictated a list. Everything else is tidied by rule and typed at once. A chat with an AI is typed as spoken.
3. **A sound on press and on release.** The first one plays once the microphone really is recording. Switch them off under Settings, General, Audio Feedback.
4. **The pill is glass.**
5. **The Mac's own microphone is used** unless you choose another under Settings, or the lid is closed. It opens fastest.
6. **Your private word file.** Names and terms in `~/Library/Logs/co.stowey.voice/vocabulary.txt`, one per line, are spelt right. They stay on your Mac.

Updating from an earlier build: quit Stowey Voice, drag the new one into Applications and replace the old one, then do steps 2 and 3 above once more. macOS will also ask for Accessibility again. Your settings and history stay.

## Good to know

Cleanup runs on Apple Intelligence, on the Mac itself. That needs an Apple silicon Mac on macOS 26 or newer with Apple Intelligence switched on. On other Macs dictation still works and the house style gate still runs, without the model.
