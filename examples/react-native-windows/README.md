# Windows text selection demo

This React Native Windows Fabric app renders two Markdown paragraphs through the public `<Supramark selection={…}>` API. The fixture supplies a parsed AST, so the demo does not require a native parser DLL. After a drag, choose **Show selection** to inspect the plain text returned by the renderer.

On Windows, install the React Native Windows 0.84 development prerequisites, including Visual Studio 2026 with the Windows C++ workload, the Windows SDK and Node.js 22.11 or newer. From the Supramark repository root, install the workspace sources and the demo’s isolated native dependencies:

```sh
bun install --frozen-lockfile --ignore-scripts
cd examples/react-native-windows
npm ci
npm run windows
```

If Metro is not already running, use `npm start` in a separate terminal. To build the JavaScript bundle independently, run `npm run bundle`.

Drag forward and backward across the paragraphs, release outside the text, and resize the window to wrap the second line. Check narrow and wide letters, CJK text and emoji. Hover over a paragraph to see the text cursor. Press **Ctrl+C** twice and paste into an editor to check that copying preserves the selection. Click the heading or the page background to dismiss it.

The demo pins React Native Windows 0.84.2 and builds its standard C++ application template with the framework from source. The patch installed by `npm ci` publishes DirectWrite line and caret positions on the existing `onTextLayout` event for Supramark's marked text views. Stock 0.84.2 omits those events. The demo owns its dependencies so the mobile example can use its existing React Native version.
