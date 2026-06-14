export function createRequire(): never {
  throw new Error('node:module is not available in the browser build');
}
