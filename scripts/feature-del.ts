#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import {
  findFeaturePackageByShortName,
  selectFeature,
  type FeaturePackageInfo,
  log,
  question,
  colors,
  closeRL,
} from './lib-feature-layout';

const REPO_ROOT = path.resolve(__dirname, '..');

interface DeleteResult {
  success: boolean;
  message: string;
}

function deleteDirectory(dirPath: string): DeleteResult {
  if (!fs.existsSync(dirPath)) {
    return { success: true, message: '目录不存在' };
  }

  try {
    fs.rmSync(dirPath, { recursive: true, force: true });
    return { success: true, message: '删除成功' };
  } catch (error) {
    return { success: false, message: error instanceof Error ? error.message : String(error) };
  }
}

interface BundleResult {
  file: string;
  success: boolean;
  message: string;
}

function removeFromBundles(featureShortName: string): BundleResult[] {
  const bundleFiles = [
    'examples/react-web-csr/src/all-features.ts',
    'examples/react-native/src/all-features.ts',
  ];

  const results: BundleResult[] = [];

  for (const bundleFile of bundleFiles) {
    const fullPath = path.join(REPO_ROOT, bundleFile);
    if (!fs.existsSync(fullPath)) {
      results.push({ file: bundleFile, success: true, message: '文件不存在' });
      continue;
    }

    try {
      let content = fs.readFileSync(fullPath, 'utf-8');

      const importRegex = new RegExp(
        `import\\s*{[^}]*}[^\\n]*from\\s*['"]@supramark/feature-${featureShortName}['"][^\\n]*\\n?`,
        'g'
      );
      content = content.replace(importRegex, '');

      const featuresArrayRegex = /(const features[^=]*=\\s*\\[)([\\s\\S]*?)(\\];)/;
      const match = content.match(featuresArrayRegex);
      if (match) {
        const arrayContent = match[2]!;
        const featureItemRegex = new RegExp(
          `\\s*[^\\n]*feature-${featureShortName}[^\\n]*,?\\s*\\n?`,
          'g'
        );
        const cleanedArrayContent = arrayContent.replace(featureItemRegex, '');

        const finalArrayContent = cleanedArrayContent
          .replace(/,\\s*\\n\\s*\\]/g, '\\n]')
          .replace(/\\n\\s*\\n/g, '\\n')
          .replace(/^\\s+|\\s+$/gm, '');

        content = content.replace(featuresArrayRegex, `$1${finalArrayContent}$3`);
      }

      fs.writeFileSync(fullPath, content, 'utf-8');
      results.push({ file: bundleFile, success: true, message: '更新成功' });
    } catch (error) {
      results.push({
        file: bundleFile,
        success: false,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return results;
}

interface DocResult {
  file: string;
  success: boolean;
  message: string;
}

function removeFromDocs(featureShortName: string): DocResult[] {
  const results: DocResult[] = [];

  const docFile = path.join(REPO_ROOT, `docs/features/${featureShortName}.md`);

  if (fs.existsSync(docFile)) {
    try {
      fs.unlinkSync(docFile);
      results.push({
        file: `docs/features/${featureShortName}.md`,
        success: true,
        message: '删除成功',
      });
    } catch (error) {
      results.push({
        file: `docs/features/${featureShortName}.md`,
        success: false,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  } else {
    results.push({
      file: `docs/features/${featureShortName}.md`,
      success: true,
      message: '文件不存在',
    });
  }

  const indexFile = path.join(REPO_ROOT, 'docs/features/index.md');
  if (fs.existsSync(indexFile)) {
    try {
      let content = fs.readFileSync(indexFile, 'utf-8');

      const featureRegex = new RegExp(
        `###\\s*\\[@supramark/feature-${featureShortName}\\][\\s\\S]*?(?=###\\s*\\[@supramark/|##\\s*|$)`,
        'g'
      );
      content = content.replace(featureRegex, '');
      content = content.replace(/\\n\\s*\\n\\s*\\n/g, '\\n\\n');

      fs.writeFileSync(indexFile, content, 'utf-8');
      results.push({ file: 'docs/features/index.md', success: true, message: '更新成功' });
    } catch (error) {
      results.push({
        file: 'docs/features/index.md',
        success: false,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return results;
}

function showResults(
  title: string,
  results: Array<{ success: boolean; message: string; file?: string; dir?: string }>,
  showSuccess = true
): void {
  if (results.length === 0) return;

  log(`\n${title}:`, 'yellow');
  results.forEach(result => {
    const color = result.success ? (showSuccess ? 'green' : 'gray') : 'red';
    const status = result.success ? '✓' : '✗';
    const name = result.file || result.dir || '未知';
    log(`  ${status} ${name}: ${result.message}`, color);
  });
}

interface CliOptions {
  featureName: string | null;
  help: boolean;
}

function parseArgs(): CliOptions {
  const args = process.argv.slice(2);
  const options: CliOptions = {
    featureName: null,
    help: false,
  };

  for (const arg of args) {
    if (arg === '--help' || arg === '-h') {
      options.help = true;
    } else if (!arg.startsWith('--')) {
      options.featureName = arg;
    }
  }

  return options;
}

function showHelp(): void {
  console.log(`
${colors.bright}Supramark Feature 删除工具${colors.reset}

${colors.blue}用法：${colors.reset}
  bun run feature:del               # 交互式选择删除
  bun run feature:del <feature-name> # 直接删除指定 feature

${colors.blue}示例：${colors.reset}
  ${colors.gray}# 交互式删除${colors.reset}
  bun run feature:del

  ${colors.gray}# 直接删除指定 feature${colors.reset}
  bun run feature:del gift
`);
}

async function main(): Promise<void> {
  log('\n🗑️  Supramark Feature 删除工具\n', 'bright');

  try {
    const cliOptions = parseArgs();

    if (cliOptions.help) {
      showHelp();
      return;
    }

    let selectedFeature: FeaturePackageInfo | null = null;

    if (cliOptions.featureName) {
      const targetName = cliOptions.featureName.replace(/^feature-/, '');
      selectedFeature = findFeaturePackageByShortName(targetName);

      if (!selectedFeature) {
        log(`❌ 未找到 Feature: ${targetName}\n`, 'red');
        return;
      }
      log(`已选择 Feature: ${colors.green}${selectedFeature.shortName}${colors.reset}\n`, 'reset');
    } else {
      selectedFeature = await selectFeature('选择要删除的 Feature:');
    }

    if (!selectedFeature) {
      log('\n已取消。\n', 'yellow');
      return;
    }

    const selectedShortName = selectedFeature.shortName;

    log('\n⚠️  警告：此操作将永久删除 Feature 及其所有相关文件！\n', 'red');
    log('将要删除的内容：', 'yellow');
    log(`  • Feature 目录: ${selectedFeature.dir}`, 'gray');
    log(`  • 包名: @supramark/feature-${selectedShortName}`, 'gray');
    log(`  • 文档文件: docs/features/${selectedShortName}.md`, 'gray');

    const confirmName = await question(
      `\n请输入 Feature 名称 "${selectedShortName}" 确认删除 (或按 Enter 取消): `
    );
    if (confirmName !== selectedShortName) {
      log('\n名称不匹配，已取消删除。\n', 'yellow');
      return;
    }

    log('\n🔄 开始删除 Feature...\n', 'gray');

    const deleteResult = deleteDirectory(selectedFeature.dir);
    showResults('Feature 目录删除', [{ dir: selectedFeature.dir, ...deleteResult }]);

    const bundleResults = removeFromBundles(selectedShortName);
    showResults('Bundle 文件更新', bundleResults);

    const docResults = removeFromDocs(selectedShortName);
    showResults('文档清理', docResults);

    log('\n📝 手动操作建议:', 'yellow');
    log('  1. 运行 bun install 重新安装依赖', 'reset');
    log('  2. 运行 bun run features:sync 同步注册表', 'reset');

    log('\n✨ Feature 删除完成！\n', 'bright');
  } catch (error) {
    log(`\n❌ 错误: ${error instanceof Error ? error.message : String(error)}\n`, 'red');
    process.exit(1);
  } finally {
    closeRL();
  }
}

main();
