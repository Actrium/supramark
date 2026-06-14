# @supramark/core

Supramark 核心包：AST 类型定义、Markdown 解析与插件系统。

## 功能特性

- **AST 定义**：统一的抽象语法树类型系统
- **Markdown 解析**：基于 Rust `supramark-markdown` 的 AST v2 解析器
- **插件系统**：标准化的功能扩展接口
- **缓存优化**：LRU 缓存机制提升性能
- **跨平台支持**：面向 React Native、Web、CLI 的统一抽象

## 安装

```bash
npm install @supramark/core
```

## 快速开始

```typescript
import { parse, type SupramarkRootNode } from '@supramark/core';

// 解析 Markdown
const ast: SupramarkRootNode = await parse('# Hello **Supramark**!');
```

## 核心接口

### SupramarkFeature

定义一个 Supramark 功能扩展的完整接口，包括：

- **metadata**: 功能元信息（名称、版本、作者等）
- **syntax**: 语法定义（AST、解析规则、验证器）
- **renderers**: 多平台渲染器（RN、Web、CLI）
- **testing**: 测试定义（语法测试、渲染测试、集成测试）
- **documentation**: 文档定义（README、API、示例）

查看 [完整 API 文档](./docs/api) 了解详情。

## 示例

查看功能扩展的完整示例：

- [Admonition 功能示例](../../docs/FEATURE_INTERFACE_EXAMPLE.md)
- [插件系统设计](../../docs/PLUGIN_SYSTEM.md)

## 开发

```bash
# 构建
npm run build

# 测试
npm test

# 测试覆盖率
npm run test:coverage

# 生成 API 文档
npm run docs
```

## License

Apache-2.0
