// Type-aware ESLint config. Heavier than the base config (needs the TS type
// checker), so it runs as a separate `lint:types` pass per package rather than
// in the fast `lint` run. Extends the base rules and turns on the type-checked
// rule set plus the strict promise/await/unsafe rules as errors.
const base = require('./.eslintrc.js');

module.exports = {
  ...base,
  // Test files are often excluded from the package tsconfig, so the type-aware
  // parser cannot resolve them. They are still covered by the fast `lint` pass.
  ignorePatterns: [...base.ignorePatterns, '**/__tests__/**', '**/*.test.ts', '**/*.test.tsx'],
  parserOptions: {
    ...base.parserOptions,
    project: true,
    tsconfigRootDir: __dirname,
  },
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
    'plugin:@typescript-eslint/recommended-type-checked',
    'plugin:react/recommended',
    'plugin:react-hooks/recommended',
    'prettier',
  ],
  rules: {
    ...base.rules,
    // Promises / async correctness
    '@typescript-eslint/no-floating-promises': 'error',
    '@typescript-eslint/no-misused-promises': 'error',
    '@typescript-eslint/await-thenable': 'error',
    '@typescript-eslint/require-await': 'error',
    // Redundancy
    '@typescript-eslint/no-unnecessary-type-assertion': 'error',
    '@typescript-eslint/no-duplicate-type-constituents': 'error',
    '@typescript-eslint/no-redundant-type-constituents': 'error',
    '@typescript-eslint/restrict-template-expressions': 'error',
    // Unsafe `any` boundaries — the complement to no-explicit-any
    '@typescript-eslint/no-unsafe-argument': 'error',
    '@typescript-eslint/no-unsafe-assignment': 'error',
    '@typescript-eslint/no-unsafe-call': 'error',
    '@typescript-eslint/no-unsafe-member-access': 'error',
    '@typescript-eslint/no-unsafe-return': 'error',
    '@typescript-eslint/unbound-method': 'error',
  },
};
