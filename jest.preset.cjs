/**
 * Supramark Feature 包的共享 Jest 配置 Preset
 *
 * 用途：
 * - 为所有 Feature 包提供统一的测试配置
 * - 与 @supramark/core 的配置保持风格一致
 * - 减少各个包中的重复配置代码
 *
 * 使用方式：
 * 在 Feature 包的 jest.config.cjs 中：
 * ```javascript
 * module.exports = {
 *   ...require('../../jest.preset.cjs'),
 * };
 * ```
 */

module.exports = {
  preset: 'ts-jest',
  // 使用自定义测试环境，预先设置 localStorage mock
  testEnvironment: '<rootDir>/../../jest-environment.cjs',

  // 测试文件路径
  roots: ['<rootDir>/src', '<rootDir>/__tests__'],
  testMatch: ['**/__tests__/**/*.test.ts', '**/?(*.)+(spec|test).ts'],

  // TypeScript 和 JavaScript 转换配置
  transform: {
    '^.+\\.ts$': [
      'ts-jest',
      {
        tsconfig: {
          module: 'commonjs',
          esModuleInterop: true,
        },
        // 忽略类型错误（特别是第三方库的类型定义缺失）
        diagnostics: {
          ignoreCodes: [7016], // 忽略 "Could not find a declaration file" 错误
        },
      },
    ],
    // 转换 node_modules 中的 ESM 模块（.js 文件）
    '^.+\\.js$': [
      'ts-jest',
      {
        tsconfig: {
          module: 'commonjs',
          esModuleInterop: true,
          allowJs: true,
        },
      },
    ],
  },

  // 模块路径映射（用于解析 @supramark/core）
  moduleNameMapper: {
    '^@supramark/core$': '<rootDir>/../core/src/index.ts',
    // 处理 .js 扩展名（实际指向 .ts 文件）
    '^(\\.{1,2}/.*)\\.js$': '$1',
  },

  // 允许转换少量仍以 ESM 发布的测试依赖。
  transformIgnorePatterns: [
    'node_modules/(?!(escape-string-regexp|@types)/)',
  ],

  // 代码覆盖率配置
  collectCoverageFrom: [
    'src/**/*.ts',
    '!src/**/*.d.ts',
    '!src/**/*.test.ts',
  ],
  coveragePathIgnorePatterns: [
    '/node_modules/',
    '/dist/',
  ],

  // 可选：启用覆盖率收集（默认关闭，按需启用）
  // collectCoverage: false,
  // coverageDirectory: 'coverage',
  // coverageReporters: ['text', 'lcov'],
};
