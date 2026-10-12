/** @type {import('jest').Config} */
module.exports = {
  // Use the shared Supramark Jest preset
  // Keeps this in sync with @supramark/core's test config
  ...require('../../../jest.preset.cjs'),

  // Feature-package-specific overrides can go here
  // e.g.:
  // testEnvironment: 'jsdom', // if a DOM environment is needed
  // collectCoverage: true,     // enable coverage collection
};
