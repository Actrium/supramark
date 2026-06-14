# Vison 技术需求文档（v1）

## 一、文档说明

### 1.1 文档目的

Vison 是一套用于 **AI 聊天消息场景的纯视觉 JSON 描述规范**，用于在 Web 与 React Native（RN）环境中统一表达与渲染消息内容的视觉结构。
仅关注 UI 展示层，不承载业务语义、不包含交互逻辑、不参与数据处理。

---

### 1.2 核心定位

> Vison 是一种 **只读视觉描述协议（Visual Schema）**，用于定义消息的结构与样式，并由客户端渲染为原生 UI。

---

### 1.3 适用范围

* 场景：AI 聊天对话的输出信息流（Message Feed）
* 平台：Web、React Native
* 渲染方式：

  * Web → HTML DOM
  * RN → 原生组件树（View / Text / Image）

---

### 1.4 核心设计原则

1. **纯视觉渲染**：不定义业务语义，不参与数据处理
2. **只读为主，有限交互**：不支持输入、编辑、提交等行为，但允许基础点击等轻量交互
3. **极简结构**：限制组件与样式集合
4. **跨端一致**：结构一致、体验一致（允许排版差异）
5. **动态渲染**：支持运行时解析与追加渲染
6. **安全可控**：严格能力边界

---

## 二、整体架构

### 2.1 技术流程

AI/业务生成 JSON → 渲染器解析 → 构建组件树 → 插入消息流

---

### 2.2 分层职责

| 层级        | 职责        |
| --------- | --------- |
| 上层（AI/业务） | 生成视觉 JSON |
| Vison     | 定义视觉规范    |
| 渲染器       | 解析并渲染     |
| 平台        | 提供 UI 能力  |

---

## 三、JSON 结构规范

### 3.1 标准结构

```json
{
  "version": "1",
  "type": "container",
  "props": {},
  "style": {},
  "children": []
}
```

---

### 3.2 字段说明

* **version（必填）**：规范版本
* **type（必填）**：组件类型
* **props（可选）**：内容属性（非样式）
* **style（可选）**：视觉样式
* **children（可选）**：子节点数组（仅 container 使用）

---

## 四、组件类型（白名单）

| type      | 描述       |
| --------- | -------- |
| container | 容器       |
| text      | 文本       |
| image     | 图片       |
| markdown  | Markdown |
| divider   | 分割线      |

---

## 五、组件属性

### text

```json
{
  "type": "text",
  "props": { "text": "内容" }
}
```

### image

```json
{
  "type": "image",
  "props": {
    "src": "...",
    "width": 300,
    "aspectRatio": 1.5
  }
}
```

### markdown

```json
{
  "type": "markdown",
  "props": {
    "content": "Markdown 内容"
  }
}
```

---

## 六、样式规范（白名单）

### 布局

padding / margin / flexDirection / alignItems / justifyContent / width / height / maxWidth / minWidth / gap

### 文本

color / fontSize / fontWeight / lineHeight / textAlign

### 视觉

backgroundColor / borderRadius / borderWidth / borderColor / opacity

---

## 七、嵌套规则

* 最大嵌套深度 ≤ 5
* 仅 container 可包含 children
* 其它组件必须为叶子节点
* 禁止循环引用

---

## 八、结构复杂度限制（强制）

* 单条消息节点数 ≤ 64
* 最大 image 数 ≤ 4
* 单文本节点长度 ≤ 65536 字符

超出限制可降级或拒绝渲染

---

## 九、Image 规范（强制）

### 9.1 尺寸约束

必须满足：

* width + height
  或
* width + aspectRatio

否则必须降级或拒绝渲染

---

### 9.2 布局稳定性

图片加载不得导致布局抖动或消息跳动

---

## 十、Markdown 规范

### 10.1 语法基准

* 以 **CommonMark (Spec 0.30+)** 为标准核心语法集。

### 10.2 支持特性

* 标题 (Headings)、加粗 (Bold)、列表 (Lists)、代码块 (Code Blocks)、链接 (Links)。

### 10.3 安全限制（强制）

* **禁止 HTML**：解析器必须强制开启安全模式，过滤或转义所有 HTML 标签。
* **禁止 script**。
* **禁止 inline style**。

### 10.4 能力限制

不保证支持：

* 表格 (Tables)
* 任务列表 (Task Lists)
* 多层嵌套列表
* 其它非 CommonMark 核心的扩展语法

---

## 十一、Streaming 渲染约束

* 必须以完整 JSON 块输出
* 禁止 token 级结构更新
* 建议 ≥100ms 批量更新

---

## 十二、容错机制

* 未知 type → 忽略或降级
* 非法 style → 忽略
* 非法 props → 忽略
* JSON 异常不得阻塞渲染

---

## 十三、非功能要求

### 性能

* 单条渲染 ≤ 50ms
* 长列表流畅

### 稳定性

* 无崩溃
* 无内存泄漏

### 可维护性

* 结构稳定
* 渲染器可扩展

---

## 十四、渲染映射

### Web

container → div
text → span/p
image → img
markdown → div
divider → hr

### RN

container → View
text → Text
image → Image
markdown → View
divider → View

---

## 十五、禁止项

* 禁止业务语义组件
* 禁止交互能力
* 禁止事件
* 禁止动画（含 shadow / transform）
* 禁止复杂布局系统
* 禁止浏览器容器 / Canvas
* 禁止扩展未定义字段

---

## 十六、验收标准

* 渲染正确
* 跨端体验统一
* 性能达标
* 无交互
* 容错正常

---

## 附录：示例

```json
{
  "version": "1",
  "type": "container",
  "style": {
    "padding": 12,
    "backgroundColor": "#F5F5F5",
    "borderRadius": 8
  },
  "children": [
    {
      "type": "text",
      "props": { "text": "部署成功" },
      "style": { "fontSize": 16, "fontWeight": "bold" }
    },
    {
      "type": "text",
      "props": { "text": "服务已启动" },
      "style": { "fontSize": 14, "color": "#666" }
    }
  ]
}
```
