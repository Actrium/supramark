# CI/CD 配置说明

本项目使用 GitHub Actions 进行持续集成和持续部署。

## Workflows

### 1. CI Workflow (`.github/workflows/ci.yml`)

主要的 CI 流程，在每次 push 到 `main`/`develop` 分支或创建 PR 时触发。

#### Test & Build Job

**运行环境**：

- Ubuntu Latest
- Node.js 18.x 和 20.x（矩阵测试）

**步骤**：

1. **代码检出**：使用 `actions/checkout@v4`
2. **环境配置**：设置 Node.js 与 Bun
3. **安装依赖**：运行 `bun install --no-progress`
4. **运行测试**：执行 `bun run test:core -- --coverage`
5. **上传覆盖率**：将覆盖率报告上传到 Codecov（仅 Node 20.x）
6. **构建项目**：执行 `bun run build`
7. **质量检查**：运行 `scripts/quality-check.js`（仅 Node 20.x）

## Badges

项目 README 中的徽章：

```markdown
[![CI](https://github.com/Actrium/supramark/actions/workflows/ci.yml/badge.svg)](https://github.com/Actrium/supramark/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Actrium/supramark/branch/main/graph/badge.svg)](https://codecov.io/gh/Actrium/supramark)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
```

## 设置步骤

### 1. GitHub Secrets（可选）

如果需要上传覆盖率到 Codecov，需要设置：

- `CODECOV_TOKEN`：从 [codecov.io](https://codecov.io) 获取

### 2. GitHub Actions 权限

当前 CI 只需要读取仓库内容并上传覆盖率，不再依赖额外的文档发布权限。

## 本地测试 CI

在推送到 GitHub 之前，可以本地运行 CI 中的命令：

```bash
# 安装依赖
bun install --no-progress

# 运行测试
bun run test:core -- --coverage

# 构建
bun run build

# 质量检查
node scripts/quality-check.js
```

## 覆盖率报告

测试覆盖率会自动上传到 Codecov：

- 报告地址：`https://codecov.io/gh/Actrium/supramark`
- 覆盖率徽章已添加到 README

当前覆盖率要求（`packages/core/jest.config.cjs`）：

- Branches: 35%
- Functions: 60%
- Lines: 55%
- Statements: 55%

## 故障排查

### 测试失败

查看 GitHub Actions 日志中的测试输出，通常问题包括：

- 依赖安装失败：检查 `package.json`、`bun.lock` 与 Bun 版本
- 测试超时：检查测试代码和 jest 配置
- 覆盖率不达标：补充测试用例

### 本地与 CI 结果不一致

可能原因：

- Node.js 版本差异：CI 使用 18.x 和 20.x
- Bun 版本或锁文件不一致：CI 使用 `bun install --no-progress`
- 操作系统差异：CI 使用 Ubuntu，本地可能是 macOS/Windows

## 未来改进

- [ ] 添加 Lint 检查（ESLint + Prettier）
- [ ] 添加类型检查（`tsc --noEmit`）
- [ ] 添加 package 发布流程
- [ ] 添加 Release 自动化
- [ ] 添加性能测试
- [ ] 添加 E2E 测试

## 相关资源

- [GitHub Actions 文档](https://docs.github.com/en/actions)
- [codecov/codecov-action](https://github.com/codecov/codecov-action)
- [TypeDoc 文档](https://typedoc.org/)
