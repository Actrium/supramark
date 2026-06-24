# Diagram (D2) Feature

D2 图表支持 Feature。

- 语法：使用围栏代码块：

````markdown
```d2
a -> b
```
````

- AST：统一解析为 `diagram` 节点，`engine` 为 `d2`。
- 渲染：由 `@supramark/engines` 调用 `@actrium/d2-little-web`（Rust wasm，纯 Rust
  布局引擎）在 Web 端生成 SVG，无需外部 Graphviz 桥。

## 快速示例

```d2
user -> database: reads
database -> user: rows
```

```d2
customers: {
  alice
  bob
}
```

本包主要提供：

- 在 FeatureRegistry 中声明「D2 图表」能力；
- 通过 `createD2FeatureConfig()` 为运行时配置提供强类型入口；
- 让 Web / RN 的 diagram gating 能和其它 family 使用同一套规则。
