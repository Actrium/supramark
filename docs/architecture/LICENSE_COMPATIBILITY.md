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
| `crates/d2-little/web-wasm` → npm `@actrium/d2-little-web` | `MPL-2.0` | 同上 | |
| `crates/mermaid-little` → crate `mermaid-little` | `MIT` | mermaid-js (MIT) | 纯 Rust 重写 |
| `crates/mermaid-little/web-wasm` → npm `@actrium/mermaid-little-web` | `MIT` | 同上 | step 4 新发 |
| `crates/supramark-markdown` → crate `supramark-markdown` | **`Apache-2.0 AND MIT`** | markdown-it-rust/markdown-it (MIT) | 自有 AST v2 + parse 编排（Apache-2.0）叠加改编自 markdown-it-rust 的 parser core（MIT）；AND 表示两协议同时约束，二者均在白名单 |
| `crates/supramark-markdown/packages/web` → npm `@supramark/markdown-web` | 同上（`Apache-2.0 AND MIT`） | 同上 | wasm 封装，协议随 crate 一致 |
| `crates/plantuml-little` → crate `plantuml-little` | **`GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT`** | PlantUML (GPL-3 / LGPL-3) | 上游主动选择 5-way OR 多协议；reimplementation，目标 byte-exact parity；supramark 以 Apache-2.0 分支消费 |
| `crates/plantuml-little/packages/web` → npm `@actrium/plantuml-little-web` | 同上（5-way OR） | 同上 | npm 发布时仍保留 OR 表达式；下游可选择最宽松分支 |
| `crates/graphviz-anywhere/graphviz/` (submodule) | **`EPL-1.0`** | Graphviz (EPL-1.0 / CPL-1.0) | 不发布；只作为 link 目标；与 Apache 文件目录隔离；目前是空目录占位（待 step 4 装回 submodule） |
| `crates/graphviz-anywhere/packages/rust` (Rust wrapper) | `Apache-2.0` | 自有 | 包名 `graphviz-anywhere`，独立发 crate |
| `crates/graphviz-anywhere/packages/rust/prebuilt/` | `EPL-1.0` | Graphviz 编译产物 | 仅在 release build 时填充；运行时 link 边界 |
| `crates/graphviz-anywhere/packages/web` → npm `@actrium/graphviz-anywhere-web` | `Apache-2.0`（JS 源码）；wasm 产物 `EPL-1.0` 衍生 | 含 wasm 形式的 Graphviz | |
| `crates/graphviz-anywhere/packages/react-native` → npm `@actrium/graphviz-anywhere-rn` | `Apache-2.0` | RN bridge | |
| `crates/vison-core` | `Apache-2.0` | 自有 | |
| `packages/vison-{web,rn}` | `Apache-2.0` | 自有 | |

## 3. 兼容性分析

### 3.1 Apache-2.0 ⇆ MIT / BSD / ISC
完全兼容。可以在任意方向相互依赖，且不传染。这是 supramark 主体 + dagre（已 Apache-2.0；reimplementation 自 MIT 上游）+ mermaid-little（MIT 上游）的关系。

### 3.2 Apache-2.0 ⇆ MPL-2.0（d2-little）
单向兼容。MPL-2.0 是**文件级** copyleft：修改 MPL 文件时该文件必须保持 MPL，但可以 link 到 Apache 工程而不传染。**可被 supramark 安全消费**；但若我方向 d2-little 的 `.rs` 文件添加新代码，那些行属 MPL-2.0。

### 3.3 Apache-2.0 ⇆ plantuml-little（5-way OR 多协议）✅
**完全兼容（升级）**。upstream 维护者主动选择了 `GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT` 五选一。supramark 以 **Apache-2.0** 分支消费，**完全无传染**：

- `@supramark/feature-plantuml` 在 Apache-2.0 路径下使用 `@actrium/plantuml-little-web`，与 supramark 主体协议一致。
- 终端用户也可自由选择任一选项（包括最严格的 GPL 或最宽松的 MIT）。
- 这条路线比 ADR-001 当初设想的 LGPL-only 更宽松，对商业用户友好。

历史决策记录见 ADR-001 修订条目。

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

### ADR-001 · plantuml-little 协议路线（修订）
**Original date:** 2026-05-09
**Revised:** 2026-05-09 (when subtree-merging step 3 surfaced upstream's actual licence choice)

**Context:** PlantUML 上游为 GPL-3 / LGPL-3。plantuml-little 是 reimplementation 而非 fork，目标 byte-exact SVG parity v1.2026.2。

**Original decision:** LGPL-3.0-or-later，作为 GPL 与 MIT 之间的平衡点。

**Revised decision:** **采纳 upstream 实际选择的 5-way OR 多协议**：
`GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT`

**Why revised:** subtree 合并时发现 `crates/plantuml-little/Cargo.toml#package.license` 上游主动声明了 5 选 1 多协议（远比我们设想的 LGPL-only 宽松）。upstream 维护者已经在源头解决了 reimplementation 与商业友好性的紧张关系——supramark 直接以 Apache-2.0 分支消费即可，无须自行收紧。

**Consequences:**
- supramark 整体 Apache-2.0 链路保留。
- `@supramark/feature-plantuml` 不再需要在 README 顶部高亮 LGPL 警告；只需声明依赖在 Apache 分支下使用。
- license-check.ts 的 OR 解析支持「任一 operand 命中 allow-list 即放行」。
- ADR 不删除原决策记录，保留为历史足迹。

### ADR-002 · dagre / graphviz-anywhere 保持独立发布

### ADR-002 · dagre / graphviz-anywhere 保持独立发布

(原 ADR-002 内容；编号未变，下移因 ADR-001 增加 revision 段落)

**Date:** 2026-05-09
**Context:** `dagre` 与 `graphviz-anywhere` 对 supramark 之外的用户也有价值。是全部 internalize 为 `@supramark/*` 还是保持原 `@actrium/*` 名独立发布？
**Decision:** 独立发布。
**Why:** internalize 会断掉它们的生态杠杆——`graphviz-anywhere` 是"万能 graphviz"通用基础设施，`dagre-rs` 是 dagre.js 的纯 Rust port，两者都有比 supramark 更广的潜在受众。物理上住在一个仓库，不影响逻辑独立性。
**Consequences:** 仓库必须用 monorepo / multi-publish 工作流；`@actrium/*` 的 npm scope 与 `@supramark/*` 并存；CI 需支持按 sub-crate 独立 release。

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
