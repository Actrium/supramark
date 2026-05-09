# 协议兼容性策略

> 总原则：**整体兼容、局部尊重上游**。
>
> supramark 是一个混合协议的超级 monorepo——主仓与 `@supramark/*` 全体走 Apache-2.0，但通过 git subtree 合并进来的多个 Rust port 仓库各自继承上游协议。本文是这套策略的权威决策记录，给后续 subtree 合并、CI 检查、发布物 `license` 字段提供对照表。

## 1. 设计原则

1. **不做 license override**。每个发布物（cargo package / npm package）的 `license` 字段以该 sub-crate 的真实 LICENSE 为准，不强行套 monorepo 默认 Apache-2.0。
2. **文件级隔离**。不同协议的源代码文件不允许 copy 复用，只允许 link / dynamic import。EPL/LGPL 文件不进入 Apache 工程目录。
3. **上游可追溯**。每个 `crates/<sub>/` 强制三件套：`LICENSE`（真实协议）+ `UPSTREAM.md`（上游来源/版本/关系/CLA 状态）+ `NOTICE`（署名）。
4. **CI 强制合规**。cargo-deny + license-checker + reuse-lint 三层守门。新增依赖若引入未登记协议必须先更新本文 + `deny.toml`，再合代码。

## 2. 协议分布矩阵（合并完成后的目标状态）

| 路径 / 发布物 | SPDX | 来源 | 备注 |
|---|---|---|---|
| `LICENSE`（仓库根） | `Apache-2.0` | 自有 | 默认协议 |
| `@supramark/core` | `Apache-2.0` | 自有 | |
| `@supramark/engines` | `Apache-2.0` | 自有 | |
| `@supramark/cli` | `Apache-2.0` | 自有 | |
| `@supramark/web` / `rn` | `Apache-2.0` | 自有 | |
| `@supramark/feature-*` | `Apache-2.0` | 自有 | |
| `crates/dagre` → crate `dagre` | `Apache-2.0` | dagre.js (Chris Pettitt, MIT) | 完整端口；reimplementation, not fork → 维护者选了 Apache-2.0 |
| `crates/d2-little` → crate `d2-little` | `MPL-2.0` | terrastruct/d2 (MPL-2.0) | 纯 Rust 端口；MPL 是文件级 copyleft，可 link |
| `crates/d2-little/web-wasm` → npm `@kookyleo/d2-little-web` | `MPL-2.0` | 同上 | |
| `crates/mermaid-little` → crate `mermaid-little` | `MIT` | mermaid-js (MIT) | 纯 Rust 重写 |
| `crates/mermaid-little/web-wasm` → npm `@kookyleo/mermaid-little-web` | `MIT` | 同上 | step 4 新发 |
| `crates/plantuml-little` → crate `plantuml-little` | **`LGPL-3.0-or-later`** | PlantUML (GPL-3 / LGPL-3) | reimplementation；目标 byte-exact parity；以 dynamic link 方式被消费 |
| `crates/plantuml-little/web-wasm` → npm `@kookyleo/plantuml-little-web` | `LGPL-3.0-or-later` | 同上 | feature-plantuml README 顶部高亮 |
| `crates/graphviz-anywhere/graphviz/` (submodule) | **`EPL-1.0`** | Graphviz (EPL-1.0 / CPL-1.0) | 不发布；只作为 link 目标；与 Apache 文件目录隔离；目前是空目录占位（待 step 4 装回 submodule） |
| `crates/graphviz-anywhere/packages/rust` (Rust wrapper) | `Apache-2.0` | 自有 | 包名 `graphviz-anywhere`，独立发 crate |
| `crates/graphviz-anywhere/packages/rust/prebuilt/` | `EPL-1.0` | Graphviz 编译产物 | 仅在 release build 时填充；运行时 link 边界 |
| `crates/graphviz-anywhere/packages/web` → npm `@kookyleo/graphviz-anywhere-web` | `Apache-2.0`（JS 源码）；wasm 产物 `EPL-1.0` 衍生 | 含 wasm 形式的 Graphviz | |
| `crates/graphviz-anywhere/packages/react-native` → npm `@kookyleo/graphviz-anywhere-rn` | `Apache-2.0` | RN bridge | |
| `crates/vison-core` | `Apache-2.0` | 自有 | |
| `packages/vison-{web,rn}` | `Apache-2.0` | 自有 | |

## 3. 兼容性分析

### 3.1 Apache-2.0 ⇆ MIT / BSD / ISC
完全兼容。可以在任意方向相互依赖，且不传染。这是 supramark 主体 + dagre（已 Apache-2.0；reimplementation 自 MIT 上游）+ mermaid-little（MIT 上游）的关系。

### 3.2 Apache-2.0 ⇆ MPL-2.0（d2-little）
单向兼容。MPL-2.0 是**文件级** copyleft：修改 MPL 文件时该文件必须保持 MPL，但可以 link 到 Apache 工程而不传染。**可被 supramark 安全消费**；但若我方向 d2-little 的 `.rs` 文件添加新代码，那些行属 MPL-2.0。

### 3.3 Apache-2.0 ⇆ LGPL-3.0-or-later（plantuml-little）⚠️
有条件兼容。LGPL 允许通过 dynamic link / wasm import 方式被非 LGPL 工程使用，被使用方不被传染。但：

- `@supramark/feature-plantuml` 通过 `import('@kookyleo/plantuml-little-web')` 动态加载 wasm，属于"独立可替换组件"，符合 LGPL 例外。
- `@supramark/core`、`@supramark/engines` 不能 transitive depend on 任何 LGPL 包。
- 终端用户若要静态链接 / 嵌入 plantuml-little，需要遵守 LGPL 条款（提供再链接能力）。
- README 顶部高亮：`feature-plantuml` 引入了 LGPL-3.0-or-later 依赖，请商业用户评估。

### 3.4 Apache-2.0 ⇆ EPL-1.0（graphviz-anywhere）⚠️
有摩擦。EPL-1.0 与 Apache-2.0 的 patent grant 条款不完全兼容（EPL 1.0 早于 Apache 2.0 的 patent grant 设计）。对策：

- **物理隔离**：`crates/graphviz-anywhere/native-c/` 整目录 EPL-1.0，不与 Apache 文件混用。
- **接口隔离**：通过 C ABI / wasm 边界消费，不在源码层 link Apache 与 EPL 的 `.rs` 文件。
- **wrapper 双协议**：`crates/graphviz-anywhere/core/` 的 Rust wrapper 是自有代码，声明 `Apache-2.0 OR MIT` 给生态最大灵活性。
- 若未来 graphviz 上游升级到 EPL-2.0，整体兼容性会变好（EPL-2.0 显式 SPDX 兼容声明）。

### 3.5 GPL-3 / AGPL（拒绝）
本仓**禁止**任何 transitive 引入纯 GPL（非 LGPL）或 AGPL 协议的依赖。例外仅在：
1. 该依赖是 build-time only（不进入发布产物）；
2. 在 `deny.toml` 显式 exception；
3. 本文档新增决策记录。

## 4. 决策记录（ADR-style）

### ADR-001 · plantuml-little 走 LGPL-3.0-or-later
**Date:** 2026-05-09
**Context:** PlantUML 上游为 GPL-3 / LGPL-3。plantuml-little 是 reimplementation 而非 fork，目标 byte-exact SVG parity v1.2026.2，意味着可能与上游有 metric/常数对齐。三个候选：MIT（最宽松）、LGPL-3.0-or-later（中间）、GPL-3.0-or-later（最保守）。
**Decision:** **LGPL-3.0-or-later**。
**Why:** MIT 路线在"byte-exact parity"语义下法律风险偏高；GPL 的传染性会让 npm/wasm 商业用户不敢碰。LGPL 是平衡点：以 dynamic link / wasm import 方式被消费时不传染主仓，对 plantuml-little 自身的修改保持 LGPL 贡献回流的对称性。
**Consequences:** `@supramark/feature-plantuml` 顶部 README 必须高亮 LGPL 属性；`@supramark/core` / `engines` 不允许 transitive 引入。

### ADR-002 · dagre / graphviz-anywhere 保持独立发布
**Date:** 2026-05-09
**Context:** `dagre` 与 `graphviz-anywhere` 对 supramark 之外的用户也有价值。是全部 internalize 为 `@supramark/*` 还是保持原 `@kookyleo/*` 名独立发布？
**Decision:** 独立发布。
**Why:** internalize 会断掉它们的生态杠杆——`graphviz-anywhere` 是"万能 graphviz"通用基础设施，`dagre-rs` 是 dagre.js 的纯 Rust port，两者都有比 supramark 更广的潜在受众。物理上住在一个仓库，不影响逻辑独立性。
**Consequences:** 仓库必须用 monorepo / multi-publish 工作流；`@kookyleo/*` 的 npm scope 与 `@supramark/*` 并存；CI 需支持按 sub-crate 独立 release。

### ADR-003 · git subtree 真合并保留上游历史
**Date:** 2026-05-09
**Context:** 6 个外部仓库合并方式：subtree（真合并） / submodule（指针） / 仅 npm 依赖。
**Decision:** git subtree 真合并。
**Why:** 现状已是"通过 npm registry 串起来的隐式 monorepo"——subtree 让物理形态匹配逻辑形态，跨 repo refactor 成本最低，CI 一次跑通。submodule 在协同 bump 时太繁琐。
**Consequences:** 首次合并需要解决目录冲突 + git 历史一次性膨胀；UPSTREAM.md 必须记录 pinned upstream commit/version 以便后续 sync。

## 5. 工具链

| 工具 | 配置 | 守门时机 |
|---|---|---|
| **REUSE** | `REUSE.toml` | `bun run license:check`（reuse lint），CI quality job |
| **license-checker-rseidelsohn** | `bun run license:check`（npm 树） | 同上 |
| **cargo-deny** | `deny.toml` | step 2 起，CI 单独 job（rust 工具链） |

## 6. 上游 sync / 回流流程

每个 `crates/<sub>/UPSTREAM.md` 记录：
- Upstream URL + pinned commit/version
- 关系（fork / reimplementation / bindings）
- 是否 copy 上游源码（"copied" 行为受上游协议约束）
- 我方协议（必须与上游兼容）
- Sync cadence（每月 / 仅安全补丁 / 不再跟进）
- 是否需要 CLA（影响是否能回流我方修改）

子项目内对上游同源代码的 patch 走 `upstream-sync/<name>` 分支，便于 rebase 上游新版本。
