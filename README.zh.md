# plantuml-little

中文 | [English](README.md)

[PlantUML](https://plantuml.com/) 的轻量级 Rust 重新实现，目标是与 Java PlantUML **v1.2026.2** 产生 **逐字节一致的 SVG 输出**。

## 这是什么

plantuml-little 读取 `.puml` 源文本，输出 `.svg` — 与 Java PlantUML 功能相同，但以原生 Rust 库 + CLI 形态运行，无需 JVM。所有支持的图表类型均通过 reference test 与上游 Java 输出逐字节对比验证。

## 对齐状态

| | |
|---|---|
| **上游版本** | PlantUML v1.2026.2 (`bb8550d`) |
| **Reference 测试** | 337 通过 / 0 基线固定 / 3 忽略（详见 `tests/known_failures.txt`） |
| **单元测试** | 2,693 |
| **集成测试** | 185 |
| **总计** | **3,215** |

### 测试方法论

Reference 测试对 `tests/fixtures/` 下每个 puml 用例的实际 SVG 输出与上游 Java 生成的 reference 做逐字节比对。为了让这种比对在不同宿主上依然一致，我们把两个会飘的维度都固定下来：

- **共享的 wasm Graphviz**。Java reference 生成管线与 plantuml-little 的 Rust 布局都把 `dot` 调用走同一份 [`@kookyleo/graphviz-anywhere-web`](https://www.npmjs.com/package/@kookyleo/graphviz-anywhere-web)（Graphviz 14.1.5 + libexpat，编译到 wasm）。Java 侧通过 `scripts/wasm-dot-wrapper.sh` 这个 shim 设为 `GRAPHVIZ_DOT`；Rust 侧通过 `PLANTUML_LITTLE_TEST_BACKEND=wasm` 启用。Graphviz 输出因此在任何机器上都位一致。
- **DejaVu Sans 字体**。plantuml-little 把 DejaVu Sans / DejaVu Sans Mono 的字宽指标烘焙进 `src/font_data.rs`，**包括对应的 `*-Oblique` 斜体面**，让 `«stereotype»` 这类 italic 字符串与 Java（在装了 `fonts-dejavu-extra` 的系统上）测量完全一致。Reference SVG 通过 `regenerate-refs.yml`（手动触发）在 Ubuntu 上刷新——那里 Java 的 `sans-serif` 经 fontconfig 解析到 DejaVu，`textLength` 字节级对齐。

Graphviz 有两种执行模式：

- `native`（默认）：链接 [`graphviz-anywhere`](https://github.com/kookyleo/graphviz-anywhere) 的预编译 `libgraphviz_api`，速度快、无需 Node；日常 `cargo test --lib` / 开发调试推荐此模式。
- `wasm`（通过 `PLANTUML_LITTLE_TEST_BACKEND=wasm` 开启）：启动与 Java reference 管线相同的 Node/wasm runner；CI 的 `test-reference` 任务采用这种模式以保证跨平台可重放。

当前基线对应的完整环境快照见 `tests/reference/VERSION`（jar 版本、JDK、Graphviz、字体栈）。

## 支持的图表类型（29 种完整实现）

全部与 Java PlantUML v1.2026.2 的 SVG 输出逐字节一致。

| 类型 | 起始标签 | 布局引擎 |
|------|----------|----------|
| Class（类图） | `@startuml` | Graphviz (Smetana) |
| Sequence（序列图） | `@startuml` | 内置引擎 (Puma / Teoz) |
| Activity v3（活动图） | `@startuml` | 内置引擎 |
| State（状态图） | `@startuml` | Graphviz |
| Component / Deployment（组件 / 部署图） | `@startuml` | Graphviz |
| Use Case（用例图） | `@startuml` | Graphviz |
| Object（对象图） | `@startuml` | Graphviz |
| Timing（时序图） | `@startuml` | 内置引擎 |
| ERD (Chen)（ER 图） | `@startchen` | Graphviz |
| Gantt（甘特图） | `@startgantt` | 内置引擎 |
| JSON | `@startjson` | 内置引擎 |
| YAML | `@startyaml` | 内置引擎 |
| Mindmap（思维导图） | `@startmindmap` | 内置引擎 |
| WBS（工作分解） | `@startwbs` | 内置引擎 |
| NWDiag（网络图） | `@startnwdiag` | 内置引擎 |
| Salt / Wireframe（线框图） | `@startsalt` | 内置引擎 |
| DOT | `@startdot` | Graphviz 透传 |
| EBNF | `@startebnf` | 内置引擎 |
| Regex（正则可视化） | `@startregex` | 内置引擎 |
| BPM（业务流程） | `@startbpm` | 内置引擎 |
| Board（看板） | `@startboard` | 内置引擎 |
| Chronology（年表） | `@startchronology` | 内置引擎 |
| Chart（图表） | `@startchart` | 内置引擎 |
| Pie（饼图） | `@startpie` | 内置引擎 |
| HCL | `@starthcl` | 内置引擎 |
| Flow（流程图） | `@startflow` | 内置引擎 |
| Wire（接线图） | `@startwire` | 内置引擎 |
| Archimate（架构图） | `@startuml` | Graphviz |
| Packet（报文结构） | `@startpacket` | 内置引擎 |

### 附加类型（文本 / 透传）

| 类型 | 说明 |
|------|------|
| Creole | `@startcreole` — 富文本标记渲染 |
| Def | `@startdef` — 纯文本显示 |
| Math / LaTeX | `@startmath` / `@startlatex` — 公式占位（Java 需外部工具） |
| Git | `@startgit` — Git 日志可视化 |
| Files | `@startfiles` — 文件树展示 |

### 明确不支持

| 类型 | 原因 |
|------|------|
| DITAA | Java 委托给第三方光栅化器（无 SVG 模式），从零实现 ASCII art → SVG 不在范围内 |
| JCCKIT | Java AWT 专属图表库，仅输出 `Graphics2D`，无 Rust 对等实现 |
| Project (Gantt v2) | Java stable v1.2026.2 自身亦不支持此类型 |

## 功能特性

- **完整预处理器**：变量、函数、条件、循环、包含、主题、35+ 内置函数
- **Skinparam 样式系统**，内置 rose 默认主题
- **Creole 富文本**：粗体 / 斜体 / 下划线 / 删除线 / 颜色 / 字体 / 链接 / 表格 / 列表
- **SVG Sprite 嵌入**，viewBox 感知缩放
- **OpenIconic 图标**（`<&icon>` 语法，223 个内置图标）
- **手绘模式**（`skinparam handwritten true`）
- **渐变填充**（线性 / 径向）
- **序列图**：8 种参与者形状、8+ 种组合片段、分隔符、自动编号
- **活动图**：泳道、goto/label 跳转、break 退出、backward 反向循环
- **状态图**：fork/join、choice、history、并发区域
- **CJK / Unicode** 字符宽度计算
- **错误报告**：行号 / 列号定位

详见 [FEATURES.md](FEATURES.md) 完整支持清单。

## 用法

```bash
# CLI
plantuml-little input.puml -o output.svg

# 库
let svg = plantuml_little::convert(puml_source)?;
```

## 前置条件

- Rust 1.82+
- [`graphviz-anywhere`](https://github.com/kookyleo/graphviz-anywhere) 预编译原生库（CI 自动拉取；本地将 `GRAPHVIZ_ANYWHERE_DIR` 指向解压后的 release tarball）
- 启用 wasm 测试后端（`PLANTUML_LITTLE_TEST_BACKEND=wasm`）时：Node 22+，并执行 `cd tests/support && npm install`
- 本地重新生成 reference SVG 时：JDK 21+、DejaVu Sans 字体（Linux 上 `apt install fonts-dejavu-core`）、以及一份 `plantuml-1.2026.2.jar`——或直接使用 `regenerate-refs.yml` 工作流在 `ubuntu-latest` 上一键搞定

## 不在范围内

- GUI、Web Server、FTP、Pipe 模式
- SVG 以外的输出格式（无 PNG / PDF / EPS / ASCII）
- PlantUML Server URL 编解码
- ELK 布局引擎
- 安全沙箱系统

## 致谢

本项目是 [PlantUML](https://plantuml.com/) 的独立 Rust 重新实现，原作者为 Arnaud Roques。我们对 PlantUML 团队在 diagram-as-code 领域的贡献深表敬意。本项目完全跟进 PlantUML 的 License 方案。

我们会不定期跟进上游的更新，所有规范性内容以上游为标准。欢迎提 Issue 和 PR。

## 许可证

PlantUML 采用多许可证开源方案。作为遵循相同语言规范的重新实现，本项目同样采用多许可证方式 — 你可以选择最适合的一种：

- [GPL 许可证](https://www.gnu.org/licenses/gpl-3.0.html)
- [LGPL 许可证](https://www.gnu.org/licenses/lgpl-3.0.html)
- [Apache 许可证](https://www.apache.org/licenses/LICENSE-2.0)
- [Eclipse Public 许可证](https://www.eclipse.org/legal/epl-2.0/)
- [MIT 许可证](https://opensource.org/licenses/MIT)

上游许可详情参见 [PlantUML 许可 FAQ](https://plantuml.com/en/faq#ddbc9d04378ee462)。
