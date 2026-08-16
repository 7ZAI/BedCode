#!/usr/bin/env node
/**
 * 一键修改项目版本号（发布用）。
 *
 * 同步更新桌面端 + 移动端各 3 处版本配置：
 *   - package.json            → "version"
 *   - src-tauri/tauri.conf.json → "version"
 *   - src-tauri/Cargo.toml    → [package] version
 *
 * 插件（plugins/、packages/）版本各自维护，不在本脚本范围内。
 *
 * 用法：
 *   node scripts/bump-version.mjs --patch            # 当前版本 +1 patch（默认）
 *   node scripts/bump-version.mjs --minor            # +1 minor
 *   node scripts/bump-version.mjs --major            # +1 major
 *   node scripts/bump-version.mjs -v 1.2.3           # 指定版本号
 *   node scripts/bump-version.mjs --desktop          # 只改桌面端
 *   node scripts/bump-version.mjs --mobile           # 只改移动端
 *   node scripts/bump-version.mjs --commit           # 修改后提交 git
 *   node scripts/bump-version.mjs --tag              # 提交并打 vX.Y.Z tag（发布用）
 *   node scripts/bump-version.mjs --dry-run          # 预览，不写文件
 *
 * 版本号来源：bedcode-desktop/src-tauri/tauri.conf.json（以此为基准递增）。
 */

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { execFileSync } from 'node:child_process';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

// ==================== 参数解析 ====================
const args = process.argv.slice(2);
const opts = {
  inc: 'patch',        // patch | minor | major
  explicit: null,      // 显式版本号
  scope: 'both',       // desktop | mobile | both
  commit: false,
  tag: false,
  dryRun: false,
};

for (let i = 0; i < args.length; i++) {
  const a = args[i];
  switch (a) {
    case '--patch': case '--minor': case '--major': opts.inc = a.slice(2); break;
    case '-v': case '--version':
      opts.explicit = args[++i];
      if (!opts.explicit) fail('--version 需要版本号参数');
      break;
    case '--desktop': opts.scope = 'desktop'; break;
    case '--mobile': opts.scope = 'mobile'; break;
    case '--both': opts.scope = 'both'; break;
    case '--commit': opts.commit = true; break;
    case '--tag': opts.tag = true; break;
    case '--dry-run': opts.dryRun = true; break;
    case '-h': case '--help': printUsage(); process.exit(0);
    default: fail(`未知参数: ${a}`);
  }
}

// ==================== 工具函数 ====================
function fail(msg) {
  console.error(`[bump-version] 错误: ${msg}`);
  process.exit(1);
}

function printUsage() {
  const text = readFileSync(new URL(import.meta.url), 'utf8');
  console.log(text.split('/**')[1].split('*/')[0].trim());
}

const SEMVER_RE = /^\d+\.\d+\.\d+$/;

function bump(version, inc) {
  const [major, minor, patch] = version.split('.').map(Number);
  switch (inc) {
    case 'major': return `${major + 1}.0.0`;
    case 'minor': return `${major}.${minor + 1}.0`;
    default: return `${major}.${minor}.${patch + 1}`;
  }
}

// 各文件对应版本条目（正则精确匹配单处出现，避免误伤依赖版本等）
const FILE_PATTERNS = {
  'package.json': /^(\s*"version"\s*:\s*")[^"]+(")/m,
  'src-tauri/tauri.conf.json': /^(\s*"version"\s*:\s*")[^"]+(")/m,
  'src-tauri/Cargo.toml': /^(\[package\]\s*[\s\S]*?^version\s*=\s*")[^"]+(")/m,
};

// ==================== 主流程 ====================
const apps = opts.scope === 'both' ? ['bedcode-desktop', 'bedcode-mobile'] : [`bedcode-${opts.scope}`];

// 当前版本以桌面端 tauri.conf.json 为基准
const currentFile = join(ROOT, 'bedcode-desktop', 'src-tauri', 'tauri.conf.json');
const currentMatch = readFileSync(currentFile, 'utf8').match(/^\s*"version"\s*:\s*"([^"]+)"/m);
if (!currentMatch) fail(`无法从 ${currentFile} 读取当前版本号`);
const current = currentMatch[1];

if (!SEMVER_RE.test(current)) fail(`当前版本号格式非法: ${current}`);

const next = opts.explicit ?? bump(current, opts.inc);
if (!SEMVER_RE.test(next)) fail(`新版本号格式非法: ${next}（应为 x.y.z）`);
if (opts.explicit && !opts.dryRun) {
  const cmp = (a, b) => a.split('.').map(Number).reduce((acc, n, i) => acc || n - b.split('.')[i], 0);
  if (cmp(next, current) < 0) console.warn(`[bump-version] 警告: 新版本 ${next} 低于当前版本 ${current}`);
}

console.log(`当前版本: ${current}  →  新版本: ${next}（范围: ${apps.join(' + ')}）`);

// 收集需要修改的文件
const targets = [];
for (const app of apps) {
  for (const [file, regex] of Object.entries(FILE_PATTERNS)) {
    const p = join(ROOT, app, file);
    const content = readFileSync(p, 'utf8');
    if (!regex.test(content)) fail(`${p} 中未找到版本条目`);
    targets.push({ path: p, file: `${app}/${file}`, content, regex });
  }
}

// 写入
if (!opts.dryRun) {
  for (const { path, file, content, regex } of targets) {
    writeFileSync(path, content.replace(regex, `$1${next}$2`));
    console.log(`  已更新: ${file}`);
  }
} else {
  for (const { file } of targets) console.log(`  [dry-run] 将更新: ${file}`);
}

// git 提交 + 打 tag（发布流程：提交后推送 tag 触发 CI）
if (!opts.dryRun && (opts.commit || opts.tag)) {
  const changed = targets.map((t) => t.path.replace(/\\/g, '/'));
  try {
    execFileSync('git', ['add', ...changed], { cwd: ROOT, stdio: 'inherit' });
    execFileSync('git', ['commit', '-m', `chore: bump version to ${next}`], { cwd: ROOT, stdio: 'inherit' });
    console.log(`  已提交: chore: bump version to ${next}`);
  } catch {
    fail('git commit 失败（请检查暂存区状态）');
  }
  if (opts.tag) {
    execFileSync('git', ['tag', `v${next}`], { cwd: ROOT, stdio: 'inherit' });
    console.log(`  已打 tag: v${next}`);
    console.log('  推送: git push origin dev --tags');
  }
}

if (opts.dryRun) console.log('（dry-run 预览，未写入任何文件）');
