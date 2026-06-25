#!/usr/bin/env node

/**
 * Supramark Feature Linter
 *
 * 检查所有 Feature 包的：
 * - 类型定义完整性
 * - 接口实现正确性
 * - 代码质量
 * - 文档完整性
 * - 测试覆盖率
 *
 * 用途：
 * - 开发时检查 Feature 质量
 * - CI/CD 中自动验证
 * - 强制统一规范
 *
 * 用法：
 *   bun run feature:lint
 *   bun run feature:lint <feature-name>
 *   bun run feature:lint -- --strict
 */

import fs from 'node:fs';
import path from 'node:path';
import {
  discoverFeaturePackages,
  selectFeature,
  type FeaturePackageInfo,
  log,
  colors,
} from './lib-feature-layout';

interface LintContext {
  packagePath: string;
  sourceCode?: string;
}

interface LintResult {
  rule: string;
  message: string;
  severity: 'error' | 'warning' | 'info';
  path: string;
}

interface LintRule {
  severity: 'error' | 'warning' | 'info';
  strictSeverity?: 'error' | 'warning' | 'info';
  message: string;
  check: (feature: ParsedFeature, context?: LintContext) => boolean;
}

interface ParsedFeature {
  metadata: {
    id?: string;
    name?: string;
    version?: string;
    description?: string;
    license?: string;
    tags?: string[];
  };
  syntax: {
    ast: {
      type?: string;
      hasSelector?: boolean;
      interface?: {
        required?: string[];
        fields?: Record<string, unknown>;
      };
      examples?: unknown[];
      multiNodeNote?: boolean;
      selector?: unknown;
    };
  };
}

const RULES: Record<string, LintRule> = {
  'metadata-id-format': {
    severity: 'error',
    message: 'Feature ID 必须符合 @scope/feature-name 格式',
    check: feature => /^@[\w-]+\/feature-[\w-]+$/.test(feature.metadata?.id ?? ''),
  },
  'metadata-version-semver': {
    severity: 'error',
    message: '版本号必须符合语义化版本格式（x.y.z）',
    check: feature => /^\d+\.\d+\.\d+$/.test(feature.metadata?.version ?? ''),
  },
  'metadata-name-required': {
    severity: 'error',
    message: 'Feature name 不能为空',
    check: feature => Boolean(feature.metadata?.name) && feature.metadata.name.length > 0,
  },
  'metadata-description-required': {
    severity: 'warning',
    message: 'Feature description 不能为空',
    check: feature => Boolean(feature.metadata?.description) && feature.metadata.description.length > 0,
  },
  'metadata-license-required': {
    severity: 'warning',
    message: 'Feature license 应该设置为 Apache-2.0',
    check: feature => feature.metadata?.license === 'Apache-2.0',
  },
  'metadata-tags-nonempty': {
    severity: 'info',
    message: 'Feature tags 建议添加至少一个标签',
    check: feature => Array.isArray(feature.metadata?.tags) && feature.metadata.tags.length > 0,
  },
  'ast-type-required': {
    severity: 'error',
    message: 'AST 节点 type 必须定义',
    check: feature => Boolean(feature.syntax?.ast?.type) && feature.syntax.ast.type.length > 0,
  },
  'ast-interface-required-nonempty': {
    severity: 'warning',
    strictSeverity: 'error',
    message: 'AST interface.required 不应只包含 type',
    check: feature => {
      const required = feature.syntax?.ast?.interface?.required;
      if (feature.syntax?.ast?.hasSelector) {
        return true;
      }
      return Array.isArray(required) && required.length > 1;
    },
  },
  'ast-interface-fields-defined': {
    severity: 'warning',
    message: 'AST interface.fields 应该定义所有 required 字段',
    check: feature => {
      const required = feature.syntax?.ast?.interface?.required || [];
      const fields = feature.syntax?.ast?.interface?.fields || {};
      return required.every(field => field in fields);
    },
  },
  'ast-examples-provided': {
    severity: 'info',
    strictSeverity: 'error',
    message: 'AST examples 应该提供至少一个示例节点',
    check: feature => {
      const examples = feature.syntax?.ast?.examples;
      return Array.isArray(examples) && examples.length > 0;
    },
  },
  'selector-multi-node-with-function': {
    severity: 'warning',
    message: '如果 Feature 处理多节点类型，应该提供 selector 函数',
    check: feature => {
      const multiNodeNote = feature.syntax?.ast?.multiNodeNote;
      const selector = feature.syntax?.ast?.selector;
      if (multiNodeNote) {
        return typeof selector === 'function';
      }
      return true;
    },
  },
  'documentation-markdown-example': {
    severity: 'warning',
    strictSeverity: 'error',
    message: 'Feature 应该在注释中提供 Markdown 使用示例',
    check: (_feature, context) => {
      if (context?.sourceCode) {
        return (
          context.sourceCode.includes('@example') && context.sourceCode.includes('```markdown')
        );
      }
      return true;
    },
  },
  'testing-file-exists': {
    severity: 'error',
    message: 'Feature 必须有测试文件',
    check: (_feature, context) => {
      if (context?.packagePath) {
        const testFile = path.join(context.packagePath, '__tests__/feature.test.ts');
        return fs.existsSync(testFile);
      }
      return true;
    },
  },
  'package-structure-complete': {
    severity: 'error',
    message: 'Feature 包必须包含所有必需文件',
    check: (_feature, context) => {
      if (!context?.packagePath) return true;

      const required = [
        'package.json',
        'tsconfig.json',
        'jest.config.cjs',
        'src/index.ts',
        'src/feature.ts',
        '__tests__/feature.test.ts',
        'README.md',
      ];

      return required.every(file => {
        const fullPath = path.join(context.packagePath, file);
        return fs.existsSync(fullPath);
      });
    },
  },
};

interface LinterOptions {
  strict: boolean;
}

class FeatureLinter {
  private strict: boolean;
  private results = {
    passed: [] as LintResult[],
    failed: [] as LintResult[],
    warnings: [] as LintResult[],
    info: [] as LintResult[],
  };

  constructor(options: LinterOptions) {
    this.strict = options.strict;
  }

  async lintFeature(featurePath: string): Promise<void> {
    log(`\n检查 Feature: ${path.basename(featurePath)}`, 'blue');
    log('─'.repeat(60), 'gray');

    const context: LintContext = {
      packagePath: featurePath,
    };

    const featureFile = path.join(featurePath, 'src/feature.ts');
    if (!fs.existsSync(featureFile)) {
      this.results.failed.push({
        rule: 'feature-file-exists',
        message: 'src/feature.ts 文件不存在',
        severity: 'error',
        path: featurePath,
      });
      log('  ❌ src/feature.ts 不存在', 'red');
      return;
    }

    const sourceCode = fs.readFileSync(featureFile, 'utf-8');
    context.sourceCode = sourceCode;

    const feature = this.extractFeatureFromSource(sourceCode);

    for (const [ruleName, rule] of Object.entries(RULES)) {
      try {
        const passed = rule.check(feature, context);

        const effectiveSeverity =
          this.strict && rule.strictSeverity ? rule.strictSeverity : rule.severity;

        const result: LintResult = {
          rule: ruleName,
          message: rule.message,
          severity: effectiveSeverity,
          path: featurePath,
        };

        if (!passed) {
          if (effectiveSeverity === 'error') {
            this.results.failed.push(result);
            log(`  ❌ ${rule.message}`, 'red');
          } else if (effectiveSeverity === 'warning') {
            this.results.warnings.push(result);
            log(`  ⚠️  ${rule.message}`, 'yellow');
          } else {
            this.results.info.push(result);
            log(`  💡 ${rule.message}`, 'blue');
          }
        } else {
          this.results.passed.push(result);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        log(`  ⚠️  规则 ${ruleName} 执行失败: ${errorMessage}`, 'yellow');
      }
    }
  }

  private extractFeatureFromSource(sourceCode: string): ParsedFeature {
    const feature: ParsedFeature = {
      metadata: {},
      syntax: { ast: { interface: {} } },
    };

    // 检测是否是新的 ContainerFeature 结构（扁平结构）
    const isContainerFeature =
      sourceCode.includes('containerNames:') || sourceCode.includes('CONTAINER_NAMES');

    if (isContainerFeature) {
      // 新结构：ContainerFeature 接口（扁平）
      // 匹配 xxxFeature: ContainerFeature = { ... } 对象
      const featureObjMatch = sourceCode.match(
        /\w+Feature:\s*ContainerFeature\s*=\s*\{([\s\S]*?)\n\};/
      );
      if (featureObjMatch) {
        const objStr = featureObjMatch[1];

        const idMatch = objStr.match(/id:\s*['"]([^'"]+)['"]/);
        if (idMatch) feature.metadata.id = idMatch[1];

        const nameMatch = objStr.match(/name:\s*['"]([^'"]+)['"]/);
        if (nameMatch) feature.metadata.name = nameMatch[1];

        const versionMatch = objStr.match(/version:\s*['"]([^'"]+)['"]/);
        if (versionMatch) feature.metadata.version = versionMatch[1];

        const descMatch = objStr.match(/description:\s*['"]([^'"]+)['"]/);
        if (descMatch) feature.metadata.description = descMatch[1];
      }

      // ContainerFeature 不需要旧的 AST 规则，标记为已满足
      feature.syntax.ast.type = 'container';
      feature.syntax.ast.hasSelector = true;
      feature.syntax.ast.interface.required = ['type', 'name', 'containerNames'];
      feature.syntax.ast.interface.fields = { type: {}, name: {}, containerNames: {} };
      feature.syntax.ast.examples = [{}];
      feature.metadata.license = 'Apache-2.0';
      feature.metadata.tags = ['container'];

      return feature;
    }

    // 旧结构：SupramarkFeature 接口（嵌套 metadata）
    const metadataMatch = sourceCode.match(/metadata:\s*{([^}]+)}/s);
    if (metadataMatch) {
      const metadataStr = metadataMatch[1];

      const idMatch = metadataStr.match(/id:\s*['"]([^'"]+)['"]/);
      if (idMatch) feature.metadata.id = idMatch[1];

      const nameMatch = metadataStr.match(/name:\s*['"]([^'"]+)['"]/);
      if (nameMatch) feature.metadata.name = nameMatch[1];

      const versionMatch = metadataStr.match(/version:\s*['"]([^'"]+)['"]/);
      if (versionMatch) feature.metadata.version = versionMatch[1];

      const descMatch = metadataStr.match(/description:\s*['"]([^'"]+)['"]/);
      if (descMatch) feature.metadata.description = descMatch[1];

      const licenseMatch = metadataStr.match(/license:\s*['"]([^'"]+)['"]/);
      if (licenseMatch) feature.metadata.license = licenseMatch[1];

      const tagsMatch = metadataStr.match(/tags:\s*\[([^\]]*)\]/);
      if (tagsMatch) {
        const tagsStr = tagsMatch[1].trim();
        feature.metadata.tags = tagsStr
          ? tagsStr.split(',').map(t => t.trim().replace(/['"]/g, ''))
          : [];
      }
    }

    const astTypeMatch = sourceCode.match(/ast:\s*{[^}]*type:\s*['"]([^'"]+)['"]/s);
    if (astTypeMatch) {
      feature.syntax.ast.type = astTypeMatch[1];
    }

    const selectorMatch = sourceCode.match(/selector:\s*\(/);
    if (selectorMatch) {
      feature.syntax.ast.hasSelector = true;
    }

    const requiredMatch = sourceCode.match(/required:\s*\[([^\]]+)\]/);
    if (requiredMatch) {
      const requiredStr = requiredMatch[1];
      feature.syntax.ast.interface.required = requiredStr
        .split(',')
        .map(f => f.trim().replace(/['"]/g, ''))
        .filter(Boolean);
    }

    const fieldsStartMatch = sourceCode.match(/fields:\s*{/);
    if (fieldsStartMatch) {
      const startIndex = fieldsStartMatch.index! + fieldsStartMatch[0].length;
      let braceCount = 1;
      let endIndex = startIndex;

      while (braceCount > 0 && endIndex < sourceCode.length) {
        if (sourceCode[endIndex] === '{') braceCount++;
        if (sourceCode[endIndex] === '}') braceCount--;
        endIndex++;
      }

      if (braceCount === 0) {
        const fieldsStr = sourceCode.substring(startIndex, endIndex - 1);
        feature.syntax.ast.interface.fields = {};

        const fieldNames = fieldsStr.match(/(\w+):\s*{/g);
        if (fieldNames) {
          fieldNames.forEach(match => {
            const name = match.match(/(\w+):/)![1];
            feature.syntax.ast.interface.fields[name] = {};
          });
        }
      }
    }

    const examplesMatch = sourceCode.match(/examples:\s*\[([^\]]*)\]/s);
    if (examplesMatch) {
      const examplesStr = examplesMatch[1].trim();
      feature.syntax.ast.examples = examplesStr ? [{}] : [];
    }

    return feature;
  }

  generateReport(): boolean {
    log('\n' + '='.repeat(60), 'gray');
    log('Feature Lint 检查报告', 'bright');
    log('='.repeat(60), 'gray');

    const total =
      this.results.passed.length +
      this.results.failed.length +
      this.results.warnings.length +
      this.results.info.length;

    log(`\n总检查项: ${total}`, 'reset');
    log(`  ✅ 通过: ${this.results.passed.length}`, 'green');
    log(
      `  ❌ 错误: ${this.results.failed.length}`,
      this.results.failed.length > 0 ? 'red' : 'green'
    );
    log(
      `  ⚠️  警告: ${this.results.warnings.length}`,
      this.results.warnings.length > 0 ? 'yellow' : 'green'
    );
    log(`  💡 建议: ${this.results.info.length}`, 'blue');

    const score = this.calculateQualityScore();
    const scoreColor = score >= 90 ? 'green' : score >= 70 ? 'yellow' : 'red';
    log(`\n质量评分: ${score}/100`, scoreColor);

    const passed = this.results.failed.length === 0;
    if (this.strict) {
      return passed && this.results.warnings.length === 0;
    }
    return passed;
  }

  private calculateQualityScore(): number {
    const total =
      this.results.passed.length +
      this.results.failed.length +
      this.results.warnings.length +
      this.results.info.length;
    if (total === 0) return 0;

    const deduction =
      this.results.failed.length * 10 +
      this.results.warnings.length * 5 +
      this.results.info.length * 2;

    return Math.max(0, 100 - deduction);
  }
}

async function main(): Promise<void> {
  log('\n🔍 Supramark Feature Linter\n', 'bright');

  const args = process.argv.slice(2);
  const strict = args.includes('--strict');

  if (args.includes('--help') || args.includes('-h')) {
    log(`
${colors.bright}用法：${colors.reset}
  bun run feature:lint              # 交互式选择要检查的 Feature
  bun run feature:lint <name>       # 检查特定 Feature 包
  bun run features:lint             # 检查所有 Features + 全局唯一性
  bun run feature:lint -- --strict  # 严格模式

${colors.blue}选项：${colors.reset}
  --strict    严格模式（警告也视为错误）
  --help, -h  显示此帮助信息

${colors.blue}示例：${colors.reset}
  ${colors.gray}# 交互式选择${colors.reset}
  bun run feature:lint

  ${colors.gray}# 检查特定 Feature${colors.reset}
  bun run feature:lint gfm

  ${colors.gray}# 检查所有 Features + containerNames 唯一性${colors.reset}
  bun run features:lint

  ${colors.gray}# 严格模式检查${colors.reset}
  bun run feature:lint -- --strict
`);
    return;
  }

  // 单个 feature 检查模式
  const linter = new FeatureLinter({ strict });
  const argFeature = args.find(arg => !arg.startsWith('--'));

  let selectedFeature: FeaturePackageInfo | null = null;

  if (!argFeature) {
    selectedFeature = await selectFeature('选择要检查的 Feature:');
    if (!selectedFeature) {
      log('\n已取消。\n', 'yellow');
      return;
    }
  } else {
    const allFeatures = discoverFeaturePackages();
    selectedFeature =
      allFeatures.find(
        item =>
          item.shortName === argFeature ||
          item.name === argFeature ||
          item.name.endsWith(`/feature-${argFeature}`)
      ) || null;
  }

  if (!selectedFeature) {
    log(`❌ 未找到 Feature: ${argFeature || '选择无效'}`, 'red');
    process.exit(1);
  }

  log(`正在检查: ${selectedFeature.shortName}...\n`, 'gray');

  await linter.lintFeature(selectedFeature.dir);

  const passed = linter.generateReport();

  process.exit(passed ? 0 : 1);
}

main().catch(error => {
  log(`\n❌ 错误: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
  console.error(error);
  process.exit(1);
});
