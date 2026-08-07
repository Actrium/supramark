# Imported Markdown fixtures

[English](README.md) | 中文

本目录只保存从外部规范或实现仓库导入的统一测试用例，不包含导入脚本、测试运行器、依赖或报告产物。

目录约定：

```text
tests/cases/
  _fixtures/
    <source-name>/
      cases.json
      cases.json.license
      version.json
      NOTICE.md
```

当前数据源：

- `commonmark`：CommonMark 0.31.2，共 652 条规范用例。
- `cmark-gfm`：GitHub cmark-gfm 0.29.0.gfm.13，共 702 条规范及扩展用例。
- `micromark`：micromark 4.0.2，共 1151 条核心 Markdown→HTML 实现回归用例。

用例的导入、校验、生产 Web Renderer 对照和中文报告工具位于
`tests/markdown-conformance/`。
