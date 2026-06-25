#!/usr/bin/env node

/**
 * Supramark Feature 增量更新工具
 *
 * 用法：
 *   bun run feature:update
 *   bun run feature:update <feature-name>
 *   bun run feature:update -- --fix
 *   bun run feature:update -- --dry-run --fix
 */

import fs from 'node:fs';
import path from 'node:path';
import {
  findFeaturePackageByShortName,
  selectFeature,
  type FeaturePackageInfo,
  log,
  colors,
} from './lib-feature-layout';

interface CheckItem {
  name: string;
  file?: string;
  check?: (pkgPath: string) => boolean;
  severity: 'critical' | 'high' | 'medium' | 'low';
  description: string;
}

const CHECKS: Record<string, CheckItem> = {
  jestConfig: {
    name: 'Jest 配置',
    file: 'jest.config.cjs',
    severity: 'high',
    description: '缺少 Jest 配置文件，测试无法运行',
  },
  tsConfig: {
    name: 'TypeScript 配置',
    file: 'tsconfig.json',
    severity: 'high',
    description: '缺少 TypeScript 配置文件',
  },
  srcIndex: {
    name: '导出入口',
    file: 'src/index.ts',
    severity: 'high',
    description: '缺少包导出入口文件',
  },
  packageJson: {
    name: 'package.json',
    file: 'package.json',
    severity: 'critical',
    description: '缺少 package.json 文件',
  },
  tsJestDep: {
    name: 'ts-jest 依赖',
    check: (pkgPath: string) => {
      const pkgJsonPath = path.join(pkgPath, 'package.json');
      if (!fs.existsSync(pkgJsonPath)) return false;
      const pkgJson = JSON.parse(fs.readFileSync(pkgJsonPath, 'utf-8'));
      const devDeps = pkgJson.devDependencies || {};
      return 'ts-jest' in devDeps;
    },
    severity: 'medium',
    description: 'package.json 中缺少 ts-jest 依赖',
  },
  multiNodeTypeGuidance: {
    name: '多节点类型指导注释',
    check: (pkgPath: string) => {
      const featurePath = path.join(pkgPath, 'src/feature.ts');
      if (!fs.existsSync(featurePath)) return false;
      const content = fs.readFileSync(featurePath, 'utf-8');
      return content.includes('多节点类型处理') || content.includes('节点类型说明');
    },
    severity: 'low',
    description: 'Feature 定义文件缺少多节点类型处理指导',
  },
};

function generateJestConfig(): string {
  return `/** @type {import('jest').Config} */
module.exports = {
  ...require('../../jest.preset.cjs'),
};
`;
}

function generateTsConfig(): string {
  return `{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "__tests__"]
}
`;
}

interface FeatureScanResult {
  name: string;
  path: string;
}

interface CheckResult {
  issues: CheckItem[];
  warnings: CheckItem[];
  suggestions: CheckItem[];
}

function checkFeaturePackage(featurePkg: FeatureScanResult): CheckResult {
  const issues: CheckItem[] = [];
  const warnings: CheckItem[] = [];
  const suggestions: CheckItem[] = [];

  for (const [key, check] of Object.entries(CHECKS)) {
    let hasProblem = false;

    if (check.file) {
      const filePath = path.join(featurePkg.path, check.file);
      hasProblem = !fs.existsSync(filePath);
    } else if (check.check) {
      hasProblem = !check.check(featurePkg.path);
    }

    if (hasProblem) {
      const item: CheckItem & { key: string } = {
        ...check,
        key,
      };

      if (check.severity === 'critical' || check.severity === 'high') {
        issues.push(item);
      } else if (check.severity === 'medium') {
        warnings.push(item);
      } else {
        suggestions.push(item);
      }
    }
  }

  return { issues, warnings, suggestions };
}

function generateReport(results: Array<{ feature: FeatureScanResult; result: CheckResult }>): {
  totalIssues: number;
  totalWarnings: number;
  totalSuggestions: number;
} {
  log('\n📊 Feature 包检查报告\n', 'bright');
  log('='.repeat(60), 'gray');

  let totalIssues = 0;
  let totalWarnings = 0;
  let totalSuggestions = 0;

  for (const { feature, result } of results) {
    const { issues, warnings, suggestions } = result;
    const hasProblems = issues.length > 0 || warnings.length > 0 || suggestions.length > 0;

    if (!hasProblems) {
      log(`\n✅ ${feature.name}`, 'green');
      log('   无需更新，所有检查通过', 'gray');
      continue;
    }

    log(
      `\n${issues.length > 0 ? '❌' : warnings.length > 0 ? '⚠️' : '💡'} ${feature.name}`,
      issues.length > 0 ? 'red' : warnings.length > 0 ? 'yellow' : 'blue'
    );
    log(`   路径: ${path.relative(process.cwd(), feature.path)}`, 'gray');

    if (issues.length > 0) {
      log('\n   🚨 关键问题：', 'red');
      issues.forEach(issue => {
        log(`      • ${issue.name}: ${issue.description}`, 'reset');
      });
      totalIssues += issues.length;
    }

    if (warnings.length > 0) {
      log('\n   ⚠️  警告：', 'yellow');
      warnings.forEach(warning => {
        log(`      • ${warning.name}: ${warning.description}`, 'reset');
      });
      totalWarnings += warnings.length;
    }

    if (suggestions.length > 0) {
      log('\n   💡 建议：', 'blue');
      suggestions.forEach(suggestion => {
        log(`      • ${suggestion.name}: ${suggestion.description}`, 'reset');
      });
      totalSuggestions += suggestions.length;
    }
  }

  log('\n' + '='.repeat(60), 'gray');
  log('\n📈 统计汇总：', 'bright');
  log(`   总包数: ${results.length}`, 'reset');
  log(`   关键问题: ${totalIssues}`, totalIssues > 0 ? 'red' : 'green');
  log(`   警告: ${totalWarnings}`, totalWarnings > 0 ? 'yellow' : 'green');
  log(`   建议: ${totalSuggestions}`, totalSuggestions > 0 ? 'blue' : 'green');

  return { totalIssues, totalWarnings, totalSuggestions };
}

async function autoFix(
  results: Array<{ feature: FeatureScanResult; result: CheckResult }>,
  options: { dryRun?: boolean } = {}
): Promise<number> {
  const { dryRun = false } = options;

  log('\n🔧 开始自动修复...\n', 'bright');

  let totalFixed = 0;
  let totalSkipped = 0;

  for (const { feature, result } of results) {
    const { issues, warnings } = result;
    const allProblems = [...issues, ...warnings];

    if (allProblems.length === 0) continue;

    log(`\n📦 ${feature.name}`, 'blue');

    let featureFixed = 0;
    let featureSkipped = 0;

    for (const problem of allProblems) {
      const filePath = CHECKS[problem.key as keyof typeof CHECKS]?.file;

      if (!filePath) {
        log(`   ⏭  跳过: ${problem.name} (需要手动处理)`, 'gray');
        featureSkipped++;
        continue;
      }

      const fullPath = path.join(feature.path, filePath);
      const relativePath = path.relative(process.cwd(), fullPath);

      if (dryRun) {
        log(`   🔍 [DRY-RUN] 将创建: ${relativePath}`, 'yellow');
        log(`      问题: ${problem.name}`, 'gray');
        continue;
      }

      try {
        let content = '';
        let action = '';

        if (problem.key === 'jestConfig') {
          content = generateJestConfig();
          action = '创建 Jest 配置';
        } else if (problem.key === 'tsConfig') {
          content = generateTsConfig();
          action = '创建 TypeScript 配置';
        } else if (problem.key === 'srcIndex') {
          const featurePath = path.join(feature.path, 'src/feature.ts');
          if (fs.existsSync(featurePath)) {
            const featureContent = fs.readFileSync(featurePath, 'utf-8');
            const exportMatch = featureContent.match(/export\s+const\s+(\w+Feature)/);
            const featureName = exportMatch ? exportMatch[1] : 'feature';

            content = `/**
 * ${feature.name
   .split('-')
   .map(w => w.charAt(0).toUpperCase() + w.slice(1))
   .join(' ')} Feature
 *
 * @packageDocumentation
 */

export { ${featureName} } from './feature.js';
`;
            action = '创建导出入口';
          } else {
            content = `export { feature } from './feature.js';\n`;
            action = '创建导出入口';
          }
        }

        if (content) {
          const dir = path.dirname(fullPath);
          if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
          }

          fs.writeFileSync(fullPath, content, 'utf-8');
          log(`   ✅ ${action}: ${relativePath}`, 'green');
          featureFixed++;
        }
      } catch (error) {
        log(
          `   ❌ 失败: ${relativePath} (${error instanceof Error ? error.message : String(error)})`,
          'red'
        );
      }
    }

    if (featureFixed > 0) {
      log(`   ─── 已修复 ${featureFixed} 项`, 'gray');
      totalFixed += featureFixed;
    }
    if (featureSkipped > 0) {
      log(`   ─── 跳过 ${featureSkipped} 项 (需手动处理)`, 'yellow');
      totalSkipped += featureSkipped;
    }
  }

  log('\n' + '='.repeat(60), 'gray');
  if (totalFixed > 0) {
    log(`\n✅ 修复完成！共修复 ${totalFixed} 项`, 'green');
  }
  if (totalSkipped > 0) {
    log(`⚠️  跳过 ${totalSkipped} 项 (需手动处理)`, 'yellow');
  }
  if (totalFixed === 0 && totalSkipped === 0) {
    log('\n✅ 无需修复', 'green');
  }

  return totalFixed;
}

async function main(): Promise<void> {
  log('\n🔍 Supramark Feature 增量更新工具\n', 'bright');

  try {
    const args = process.argv.slice(2);
    const dryRunFlag = args.includes('--dry-run');

    if (args.includes('--help') || args.includes('-h')) {
      log(`
${colors.bright}用法：${colors.reset}
  bun run feature:update              # 交互式选择，自动检查并修复
  bun run feature:update <name>       # 检查并修复特定 Feature
  bun run feature:update -- --check-only  # 仅检查，不自动修复

${colors.blue}选项：${colors.reset}
  --check-only  仅检查，不自动修复（预览模式）
  --dry-run     预览修复而不实际执行
  --help, -h    显示此帮助信息

${colors.blue}示例：${colors.reset}
  ${colors.gray}# 交互式选择并自动修复${colors.reset}
  bun run feature:update

  ${colors.gray}# 检查并修复特定 Feature${colors.reset}
  bun run feature:update gfm

  ${colors.gray}# 仅检查，不修复${colors.reset}
  bun run feature:update -- --check-only
`);
      process.exit(0);
    }

    const argFeature = args.find(arg => !arg.startsWith('--'));
    const checkOnly = args.includes('--check-only');

    let selectedFeature: FeaturePackageInfo | null = null;

    if (!argFeature) {
      selectedFeature = await selectFeature('选择要更新的 Feature:');
      if (!selectedFeature) {
        log('\n已取消。\n', 'yellow');
        return;
      }
    } else {
      selectedFeature = findFeaturePackageByShortName(argFeature);
      if (!selectedFeature) {
        log(`\n❌ 未找到 Feature: ${argFeature}\n`, 'red');
        return;
      }
    }

    log(`正在扫描 Feature: ${selectedFeature.shortName}...`, 'gray');

    const results = [
      {
        feature: { name: selectedFeature.shortName, path: selectedFeature.dir },
        result: checkFeaturePackage({ name: selectedFeature.shortName, path: selectedFeature.dir }),
      },
    ];

    const stats = generateReport(results);
    const needFix = stats.totalIssues > 0 || stats.totalWarnings > 0;

    if (needFix && !checkOnly) {
      log('\n🔧 自动修复缺失项...\n', 'bright');
      await autoFix(results, { dryRun: dryRunFlag });

      log('\n📦 重新扫描确认...\n', 'gray');
      const recheck = [
        {
          feature: { name: selectedFeature.shortName, path: selectedFeature.dir },
          result: checkFeaturePackage({
            name: selectedFeature.shortName,
            path: selectedFeature.dir,
          }),
        },
      ];
      generateReport(recheck);
    } else if (needFix) {
      log('\n💡 提示: 使用 bun run feature:update 自动修复缺失项\n', 'blue');
    }

    if (stats.totalIssues === 0 && stats.totalWarnings === 0) {
      log('\n✨ 所有检查通过，Feature 包符合最新标准！', 'green');
    }
  } catch (error) {
    log(`\n❌ 错误: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
    console.error(error);
    process.exit(1);
  }
}

main();
