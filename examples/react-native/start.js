const { spawnSync } = require('node:child_process');
const { existsSync } = require('node:fs');
const { resolve } = require('node:path');

const here = __dirname;
const rootDir = resolve(here, '../..');

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.status !== 0) {
    process.exit(result.status == null ? 1 : result.status);
  }
}

// 确保根目录依赖已安装（只在缺少 node_modules 时执行）
if (!existsSync(resolve(rootDir, 'node_modules'))) {
  console.log('[supramark/native] 未检测到根目录 node_modules，正在执行 bun install...');
  run('bun', ['install'], rootDir);
}

// 启动 Expo 开发服务器
console.log('[supramark/native] 启动 Expo（expo start）...');
run('bunx', ['expo', 'start'], here);

