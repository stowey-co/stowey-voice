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

1. **Stowey is spelt right.** The speech model and the cleanup model both know the name. Add the names of colleagues, customers and suppliers under Settings, Custom Words. They stay on your Mac.
2. **It starts listening sooner.** With AirPods or another Bluetooth headset as the Mac's input, the app now records from the Mac's own microphone, as long as the lid is open. A headset needs a second or more to switch into call mode and sends telephone quality sound. Choose a microphone under Settings to overrule this.
3. **Less waiting after you stop.** The speech model loads at launch and stays loaded, the cleanup model is warmed up while you speak, and a short sentence with nothing to clean is typed without the cleanup model.

Updating from an earlier build: quit Stowey Voice, drag the new one into Applications and replace the old one, then do steps 2 and 3 below once more. macOS will also ask for Accessibility again. Your settings and history stay.

## Good to know

Cleanup runs on Apple Intelligence, on the Mac itself. That needs an Apple silicon Mac on macOS 26 or newer with Apple Intelligence switched on. On other Macs dictation still works and the house style gate still runs, without the model.
