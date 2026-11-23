Stowey® Voice for macOS. Hold a key, speak, and clean text is typed into whatever app you are in.

## Which file

| Your Mac | File |
|---|---|
| Apple silicon (M1 or newer) | `Stowey-Voice-apple-silicon.dmg` |
| Intel | `Stowey-Voice-intel.dmg` |

## Install

1. Open the downloaded file and drag Stowey® Voice into Applications.
2. Open Stowey® Voice. macOS says it cannot verify the app. Click Done.
3. Open System Settings, then Privacy & Security. Scroll down and click Open Anyway next to Stowey® Voice. Confirm once.
4. Follow the setup inside the app. It asks for the microphone and for Accessibility, which it needs to type for you.

Step 2 and 3 are needed once per download. They exist because this build is not yet signed with an Apple Developer account.

## New in this build

1. **Pauses no longer end your sentences.** When you stop to think, the text is read as a whole and the full stops go where your meaning ends. This now applies in AI chats too.
2. **Lists come out as lists.** Say "we have three points, one is ..., two is ...", and the points arrive numbered, each on its own line.
3. **The pill shows on the screen you are working on:** the one with the window you clicked into.
4. **The pill can sit on the left or right edge**, standing upright, halfway up the screen. Settings, Advanced, Overlay Position.
5. **The sound bars move a little more.**

Longer dictations now go through the cleanup model, so they take a moment longer to appear. Short, plain ones are still typed at once.

Fn and C still copies your last dictation. If macOS opens Control Centre instead, give the shortcut another pair of keys under Settings, General.

Updating from an earlier build: drag the new one into Applications and replace the old one, then do steps 2 and 3 above once more. macOS will also ask for Accessibility again. Your settings, history and word file stay. If an app called Stowey Voice, without the mark, is still in Applications, move it to the Bin.

## Good to know

Cleanup runs on Apple Intelligence, on the Mac itself. That needs an Apple silicon Mac on macOS 26 or newer with Apple Intelligence switched on. On other Macs dictation still works and the house style gate still runs, without the model.
