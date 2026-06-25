import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import type { ContainerExtensionSpec } from '../packages/core/src/container-extension';
import { discoverFeaturePackages } from './lib-feature-layout';

async function loadExtensionSpec(featurePath: string): Promise<ContainerExtensionSpec | null> {
  const extTs = path.join(featurePath, 'src', 'extension.ts');
  if (!fs.existsSync(extTs)) return null;

  // 直接 import TS 源码：由 bunx tsx 执行，支持 ts/esm
  const mod = (await import(pathToFileURL(extTs).href)) as { extension?: ContainerExtensionSpec };
  const spec = mod.extension as ContainerExtensionSpec | undefined;
  return spec ?? null;
}

function validateSpec(spec: ContainerExtensionSpec, source: string) {
  if (spec.kind !== 'container') throw new Error(`Invalid kind in ${source}`);
  if (!spec.featureId) throw new Error(`Missing featureId in ${source}`);
  if (!spec.nodeName) throw new Error(`Missing nodeName in ${source}`);
  if (!Array.isArray(spec.containerNames) || spec.containerNames.length === 0) {
    throw new Error(`Missing containerNames in ${source}`);
  }
  if (!spec.parserExport) throw new Error(`Missing parserExport in ${source}`);
  if (!spec.webRendererExport) throw new Error(`Missing webRendererExport in ${source}`);
  if (!spec.rnRendererExport) throw new Error(`Missing rnRendererExport in ${source}`);
}

async function main() {
  const allFeatures = discoverFeaturePackages();
  const specs: ContainerExtensionSpec[] = [];

  for (const feature of allFeatures) {
    const spec = await loadExtensionSpec(feature.dir);
    if (!spec) continue;
    spec.featureDir = feature.shortName; // shortName 现在是 'admonition' 这种
    validateSpec(spec, `${feature.dir}/src/extension.ts`);
    specs.push(spec);
  }

  // 之前的 codegen 逻辑已被彻底移除。
  // 现在的架构采用“被动型”渲染器：Supramark 组件直接从 config.features 中动态解析渲染逻辑，
  // 无需在库级别维护一个全局的注册表或中间件代码。

  console.log(
    `[features:sync] Scanned ${specs.length} container extension(s). Document synchronization triggered.`
  );
}

main().catch(err => {
  console.error(err);
  process.exitCode = 1;
});
