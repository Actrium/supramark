module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  parserOptions: {
    ecmaVersion: 2022,
    sourceType: 'module',
    ecmaFeatures: {
      jsx: true,
    },
  },
  env: {
    browser: true,
    node: true,
    es2022: true,
  },
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
    'plugin:react/recommended',
    'plugin:react-hooks/recommended',
    'prettier', // 关闭与 Prettier 冲突的规则
  ],
  plugins: ['@typescript-eslint', 'react', 'react-hooks'],
  settings: {
    react: {
      version: 'detect',
    },
  },
  rules: {
    // TypeScript: type-safety
    // Explicit `any` is forbidden. Use precise types, generics, or `unknown` with a
    // narrowing guard. Untyped third-party boundaries should be wrapped in minimal interfaces.
    '@typescript-eslint/no-explicit-any': 'error',
    '@typescript-eslint/explicit-module-boundary-types': 'off',
    // Unused code is an error: dead vars/imports/params usually signal a real mistake.
    '@typescript-eslint/no-unused-vars': [
      'error',
      {
        argsIgnorePattern: '^_',
        varsIgnorePattern: '^_',
        caughtErrorsIgnorePattern: '^_',
      },
    ],
    // Prefer `import type { ... }` for type-only imports (bundler/tree-shaking friendly).
    '@typescript-eslint/consistent-type-imports': [
      'error',
      { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
    ],

    // React
    'react/react-in-jsx-scope': 'off', // React 17+ does not need it
    'react/prop-types': 'off', // TypeScript covers prop types
    'react/display-name': 'off',

    // General correctness/strictness
    'no-console': ['error', { allow: ['warn', 'error'] }],
    'prefer-const': 'error',
    'no-var': 'error',
    eqeqeq: ['error', 'always', { null: 'ignore' }],
    'no-implicit-coercion': 'error',
  },
  ignorePatterns: [
    'node_modules/',
    'dist/',
    '**/dist/**',
    '**/lib/**',
    'build/',
    '**/build/**',
    '**/output/**',
    'target/',
    '**/target/**',
    'generated/',
    '**/generated/**',
    '*.tsbuildinfo',
    '**/*.tsbuildinfo',

    // vendored/generated runtime bundles
    'crates/d2-little/mathjax.js',
    'crates/d2-little/setup.js',
    'crates/mermaid-little/src/katex/vendor/**',
    'crates/mermaid-little/src/cose_bilkent_js/**',

    // docs build outputs / caches
    'docs/public/preview/',
    'docs/public/preview/**',
    'docs/public/typedoc/',
    'docs/public/typedoc/**',
    'docs/.vitepress/cache/',
    'docs/.vitepress/cache/**',
    'packages/core/docs/api/',
    'packages/core/docs/api/**',

    '*.config.js',
  ],
  overrides: [
    {
      files: ['crates/*/packages/react-native/src/**/*.ts'],
      rules: {
        // React Native TurboModule fallbacks are intentionally resolved
        // synchronously to match the generated module contract.
        '@typescript-eslint/no-var-requires': 'off',
      },
    },
    {
      // Build/codegen/install scripts, Metro launchers and the CLI write to
      // stdout/stderr and use CommonJS require by design.
      files: [
        '**/scripts/**/*.{ts,js,cjs,mjs}',
        'examples/**/*.{ts,tsx,js}',
        'packages/cli/**/*.{ts,tsx}',
      ],
      env: { node: true },
      rules: {
        'no-console': 'off',
        '@typescript-eslint/no-var-requires': 'off',
      },
    },
  ],
};
