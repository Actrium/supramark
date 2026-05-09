// Type stub for the wasm-bindgen-generated module. The real JS + wasm
// live at ../../dist/wasm/ and are produced by `wasm-pack build
// --target bundler` — see the `build:wasm` script in package.json.
//
// We keep a stub here so `tsc` can resolve `./wasm/d2_little_web.js`
// during TypeScript compilation. At runtime (after build), the relative
// import resolves to `dist/wasm/d2_little_web.js`, which is the actual
// wasm-pack output.
export function convert(input: string): string;
export function version(): string;
