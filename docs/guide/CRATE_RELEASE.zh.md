# Rust crate 发布流程

本仓库的 Rust crate 走「修复带版本号，合并即发布」的流程。本文说明规则、命令和一次性配置。

`graphviz-anywhere` 不走这条流程：它由 [`graphviz-release.yml`](https://github.com/Actrium/supramark/blob/main/.github/workflows/graphviz-release.yml) 用 `v[0-9]*` tag 发布，发布前必须先完成跨平台构建矩阵并校验 release 产物。

## 为什么要有这条流程

在此之前，除 `graphviz-anywhere` 以外的 crate 全部靠手工 `cargo publish`：没有 tag、没有 release、没有质量门。结果是修复落到 main 之后不会自动到达 crates.io。#254 和 #255 在 2026-10-16 修完，十二天后仍未发版——下游拿到了提交，却没有可以依赖的版本号。

现在的链路：

1. **修复 PR 自带版本号**：`bun run crate:bump <crate>` 改版本并传导依赖约束；CI 的 `Crate release bookkeeping`（在 Lint job 里）在漏改时让 PR 变红。
2. **合并后自动打 tag**：`crate-auto-tag.yml` 在每次 push 到 main 时查出「manifest 版本不在 crates.io 上」的 crate，按依赖顺序逐个打 `<crate>-v<version>` tag 并派发发布流程。tag 已存在但版本仍未发布时，重新派发同一个 tag——失败会自动重试。
3. **发布单个 crate**：`crate-release.yml` 校验 tag 与 manifest 一致，跑 fmt / clippy / test，然后 `cargo publish`，并建一个 GitHub Release。

crates.io 的版本不可变，错发一个版本号就永久烧掉它。所以触发器只认 manifest 里的版本行——它只会在评审过的 PR 里变化。

## 版本号规则

仓库里有两种版本形态，自动步骤对两者都只走**最小一步**：

| 形态 | 例子 | 自动递增结果 | 说明 |
|---|---|---|---|
| 上游版本 + 修订号 | `mermaid-little 11.14.0-6` | `11.14.0-7` | 前段标明移植自哪个上游版本，只有修订号属于本仓库 |
| 普通 semver | `supramark-diagram 0.1.6` | `0.1.7` | 第一方 crate |

对 `11.14.0-6` 递增 patch 会得到 `11.14.1`，相当于声称存在一个上游 mermaid 版本——所以带数字修订号时只动修订号。

需要更大的跨度（minor、major、跟上游重新对齐）时用 `--set`：

```sh
bun run crate:bump mermaid-little --set 11.15.0-1
```

## 精确 pin 的传导

`supramark-diagram` 对两个引擎用的是精确 pin：

```toml
mermaid-little = { path = "../mermaid-little", version = "=11.14.0-6", ... }
d2-little      = { path = "../d2-little",      version = "=0.7.1-1", ... }
```

所以发 `mermaid-little` 必然连带发 `supramark-diagram`：pin 要改，而改了 pin 就改变了 `supramark-diagram` 发布出去的内容。`crate:bump` 自动完成整条链：

```console
$ bun run crate:bump mermaid-little
mermaid-little: 11.14.0-6 -> 11.14.0-7  (requested)
supramark-diagram: 0.1.6 -> 0.1.7  (follows mermaid-little 11.14.0-7)
Cargo.lock refreshed
```

caret 形式的约束（`version = "0.1"`）只要仍能匹配新版本就不动，也不连带发版。`publish = false` 的 wasm / native 包装 crate 只维护约束，没有版本要动。

## 依赖顺序与失败重试

`crate-release.yml` 在 `cargo publish` 之前会跑 `wait-deps`：如果这次发布依赖的新版本还没出现在 crates.io 上，它轮询 index 直到可解析（默认超时 15 分钟）。所以 `mermaid-little` 和 `supramark-diagram` 在同一次 push 里被发布时，后者不会抢在前者上传完成之前失败。caret 约束如果仍能解析到已发布的旧版本，则不等待。

一次发布失败（fmt / clippy / test / token / 网络）不会留下死状态：tag 留在仓库里，但版本不在 crates.io 上，下一次 push 到 main 会重新派发同一个 tag。`cargo publish` 是幂等的，已存在的版本按成功处理。

如果 manifest 的当前版本在 crates.io 上被 yank，则无法重新发布；`pending` 会提示 bump 到一个新版本。

`wait-deps` 遇到两种无法靠等待解决的情况会立即失败，而不是空等 15 分钟：树上版本连自己的约束都满足不了（stale pin，`crate:check` 也会报），或依赖由 `graphviz-release.yml` / 手工发布、本流水线永远不会产出该版本。

## 命令

```sh
bun run crate:list              # 本流程发布哪些 crate、当前版本、内部依赖
bun run crate:next <crate>      # 下一个版本号是什么
bun run crate:bump <crate>      # 改版本 + 传导 pin + 刷新 Cargo.lock
bun run crate:bump <crate> --set 1.2.3
bun run crate:check             # 本地跑一遍 CI 的记账检查
bun run crate:pending           # 哪些 crate 的版本还没上 crates.io
bun scripts/crate-release.ts wait-deps <crate>   # 发布前等待依赖版本出现在 crates.io（workflow 内部使用）
bun run crate:selftest          # 脚本自测（`bun run quality` 会跑）
```

## CI 记账检查查什么

`Crate release bookkeeping` 按「坏得多严重」排序断言三件事：

1. **工作区内部的版本约束仍能匹配树里的版本**。bump 之后残留的精确 pin 会让 `cargo publish` 失败，更糟的情况是发布成功、但指向一个即将被取代的版本。
2. **每个成员的版本与 `Cargo.lock` 一致**，避免 bump 把 lock 留给下一个 PR 去更新。
3. **打包内容发生变化的 crate，版本号与基线不同，且不在 crates.io 上**。这一条才是让「修复合并」等于「修复发版」的部分。

只改文档、测试、benches、examples 不要求 bump。确实不需要发版的改动，给 PR 打 `release:skip` 标签即可跳过整步。

## 手工发布

两种情况需要手工派发 `crate-release.yml`（Actions 页面 → Crate Release → Run workflow，填 crate 名）。**派发时把 ref 选成该 crate 的目标 tag**，否则发布的是所选 ref 的树，而不是 tag 指向的树：

- **从未发布过的 crate**（当前是 `supramark-code-highlight`、`vison-core`）。首次发布会占用 crates.io 上的名字，不可撤销，所以自动流程只跳过并提示。
- **补救一次失败的发布**。`cargo publish` 的调用是幂等的：已存在的版本按成功处理，所以重跑是安全的。

打包验证失败（而不是代码问题）时，可以在派发时勾上 `no_verify`，它给 `cargo publish` 加 `--no-verify`。

## tag 命名空间

| tag | 归属 |
|---|---|
| `<crate>-v<version>`，如 `mermaid-little-v11.14.0-7` | 本流程 |
| `v[0-9]*`，如 `v0.2.6` | `graphviz-release.yml`（graphviz-anywhere） |
| `test-v[0-9]*` | `graphviz-release.yml` 的预演，本流程显式忽略 |

给库打一个裸 `v0.1.7` 会启动 graphviz 那条跨平台构建矩阵，并以一次重复发布失败收场。所以库的 tag 一律带 crate 名前缀。`vison-core-v0.1.0` 虽然以 `v` 开头，但 `v[0-9]*` 不匹配它，因此不会误触 graphviz 矩阵。

## 一次性配置

- **`CARGO_TOKEN`**：已配置（`graphviz-release.yml` 在用同一个 secret）。它需要能发布本流程涉及的所有 crate。
- **`release:skip` 标签**：需要在仓库里创建，否则没人能用这个豁免口。

  ```sh
  gh label create release:skip -R Actrium/supramark \
    -c '#ededed' -d 'Changes a crate without needing a crates.io release'
  ```

- **必需状态检查**：记账检查放在 `Lint` job 里，而 `Lint` 已经是 main 的必需检查，不需要改 ruleset。
