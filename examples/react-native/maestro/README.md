# Selection E2E

This folder contains Maestro flows for the React Native selection demo.

Run the iOS flow from a macOS machine with a booted simulator:

```bash
MAESTRO_DEVICE_UDID=<simulator-udid> bun --filter @supramark/example-react-native e2e:selection:ios
```

Run the Android flow with an adb device or AVD:

```bash
MAESTRO_DEVICE_UDID=<adb-serial> bun --filter @supramark/example-react-native e2e:selection:android
```

The runner sets `SUPRAMARK_RN_E2E=selection`, starts the Expo dev server,
builds and installs the selection-only harness, runs the Maestro flows, captures
screenshots for gesture, CJK, and nested FlatList selection states, checks their
highlight / handle / toolbar pixels, and restores generated CocoaPods / Xcode
files before exiting on iOS. Android also covers blank-space long-press
dismissal and asserts that a selected FlatList row keeps its highlight aligned
after the list scrolls.

If macOS system proxying is enabled, make sure `localhost` and `127.0.0.1` are
in the bypass list. The iOS simulator loads the Metro bundle through that local
URL.
