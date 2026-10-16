/**
 * 前端 ESLint 锁 glob ↔ 真实目录一致性（源码扫描型锁）
 *
 * 为什么需要这条锁（2026-10-16 实测事故）：`eslint.config.js` 的 `frontendSourceGlobs`
 * 是**手写目录清单**，而目录会改名。当时 `bedcode-mobile/plugins/<app>/src/**` 对应的目录早已
 * 改名成 `wasm-apps/`，于是 `pnpm exec eslint .` 全绿、CI 的 lint job 全绿，但
 * **移动端全部 wasm 应用的前端零资源访问锁（fetch / WebSocket / XMLHttpRequest /
 * plugin-http 等，AGENTS.md §6 前端红线）完全没有挂在文件上**——`--print-config`
 * 对移动文件返回这些规则 `off`。手写清单的漂移不会让 lint 报错，只会让锁静默消失。
 *
 * 判据（把「glob 是否指向真实目录」变成可测事实）：
 * | 契约ID | 来源 | 行为/规则 | 预期 |
 * | --- | --- | --- | --- |
 * | C-1 | `frontendSourceGlobs` 每项 | 字面前缀（第一个 `*`/`{` 之前）必须是真实目录 | 违规 = 点名该 glob |
 * | C-2 | 同上 | 每个 glob 必须至少匹配到一个真实源文件（防指向空目录仍绿） | 违规 = 点名该 glob |
 * | C-3 | 两端 `wasm-apps/<app>/src` | 每个含 `src/` 的 wasm 应用都要被某条 glob 覆盖 | 违规 = 点名该 app 目录 |
 * | C-4 | 数组本身 | `frontendSourceGlobs` 抽取不到 / 为空 ⇒ 抛错（不许静默通过） | 抛错 |
 *
 * fs 访问经参数注入（`io`），因此上表可以用**合成输入**逐个证伪：
 * 把历史形态 `bedcode-mobile/plugins/<app>/src/**` 喂进去必须报违规——若该断言不成立，
 * 说明锁对「改名后 glob 漂移」是瞎的（这正是要防的那一类回归）。
 *
 * 与 ESLint 的分工：ESLint 是运行期门禁（`lint.yml` 的 `pnpm exec eslint .`）；
 * 本文件是**配置结构门禁**——lint 无法自证「我的 glob 指的是真目录」，只有这里能。
 */

import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, dirname, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url))
const CONFIG_REL = 'eslint.config.js'
const ENDS = ['bedcode-desktop', 'bedcode-mobile']

// 遍历时跳过的生成物 / 依赖目录（本锁只关心源码在场，不关心产物）
const SKIP_DIRS = new Set(['node_modules', 'dist', 'dist-ssr', 'target', 'coverage', '.git'])

/** 从 `eslint.config.js` 源文本抽取 `frontendSourceGlobs` 数组的字面量 */
export function extractGlobs(configSource) {
  // 先剥掉 `//` 行注释，避免注释里的括号 / 引号干扰（配置里注释很多）
  const stripped = configSource
    .split('\n')
    .map((line) => line.replace(/(^|[^:"'`])\/\/[^\n]*/g, '$1'))
    .join('\n')

  const marker = 'const frontendSourceGlobs = ['
  const start = stripped.indexOf(marker)
  if (start === -1) {
    throw new Error(
      `抽取不到 \`frontendSourceGlobs\` 数组（eslint.config.js 改名 / 删除？）——` +
        `本锁的覆盖面会静默归零，故必须抛错而不是跳过`,
    )
  }
  const end = stripped.indexOf(']', start)
  if (end === -1) throw new Error('`frontendSourceGlobs` 数组未闭合（解析失败，本锁不猜）')

  const body = stripped.slice(start + marker.length, end)
  const globs = [...body.matchAll(/'([^']+)'/g)].map((m) => m[1])
  if (globs.length === 0) {
    throw new Error('`frontendSourceGlobs` 为空 —— 空清单等于没有锁，本锁必须失效报错')
  }
  return globs
}

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/**
 * glob → RegExp（只支持本配置实际用到的形态：`**` 跨目录、`*` 单段、`{a,b}` 或列表、字面量）
 *
 * 不完整的形态（未闭合的 `{`）直接抛错：猜着解析等于把覆盖面丢掉还照绿。
 */
export function globToRegExp(glob) {
  let re = ''
  for (let i = 0; i < glob.length; i++) {
    const ch = glob[i]
    if (ch === '*') {
      if (glob[i + 1] === '*') {
        if (glob[i + 2] === '/') {
          re += '(?:[^/]+/)*'
          i += 2
        } else {
          re += '.*'
          i += 1
        }
      } else {
        re += '[^/]*'
      }
    } else if (ch === '?') {
      re += '[^/]'
    } else if (ch === '{') {
      const close = glob.indexOf('}', i)
      if (close === -1) throw new Error(`glob 里的 \`{\` 未闭合：${glob}`)
      re += `(?:${glob
        .slice(i + 1, close)
        .split(',')
        .map(escapeRe)
        .join('|')})`
      i = close
    } else {
      re += escapeRe(ch)
    }
  }
  return new RegExp(`^${re}$`)
}

/** glob 的字面前缀目录（第一个通配符 / 花括号之前的目录部分） */
export function globBaseDir(glob) {
  const idx = glob.search(/[*{]/)
  const head = idx === -1 ? dirname(glob) : glob.slice(0, idx)
  return head.replace(/\/+$/, '')
}

/** 递归收集目录下的文件（POSIX 相对路径），生成物目录不进扫描面 */
export function listFiles(rootDir, base = rootDir) {
  let entries
  try {
    entries = readdirSync(rootDir, { withFileTypes: true })
  } catch {
    return []
  }
  const out = []
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue
      out.push(...listFiles(join(rootDir, entry.name), base))
    } else if (entry.isFile()) {
      out.push(relative(base, join(rootDir, entry.name)).split(sep).join('/'))
    }
  }
  return out
}

function isDir(p) {
  try {
    return statSync(p).isDirectory()
  } catch {
    return false
  }
}

/**
 * 校验 glob 清单，返回违规说明（空数组 = 通过）
 *
 * `io` 注入真实 fs（`{ existsDir, filesUnder }`），便于用合成输入做变异自检。
 */
export function validateGlobs(globs, io) {
  if (globs.length === 0) throw new Error('glob 清单为空 —— 本锁失效，必须报错')

  const violations = []
  const matchers = globs.map((glob) => {
    const base = globBaseDir(glob)
    if (!io.existsDir(join(REPO_ROOT, base))) {
      violations.push(`C-1 glob 指向不存在的目录：\`${glob}\` → \`${base}\` 不在磁盘上（目录改名后未同步清单？）`)
    }
    let re
    try {
      re = globToRegExp(glob)
    } catch (e) {
      violations.push(`C-1 glob 形态非法：\`${glob}\`（${e.message}）`)
      return { glob, re: null }
    }
    const files = io.filesUnder(join(REPO_ROOT, base))
    if (files.length > 0 && !files.some((f) => re.test(`${base}/${f}`))) {
      violations.push(`C-2 glob 匹配不到任何真实源文件：\`${glob}\`（base=\`${base}\` 下 ${files.length} 个文件）`)
    }
    return { glob, re }
  })

  // C-3：两端每个含 src/ 的 wasm 应用都必须被某条 glob 覆盖
  for (const end of ENDS) {
    const appsRoot = join(REPO_ROOT, end, 'wasm-apps')
    let apps = []
    try {
      apps = readdirSync(appsRoot, { withFileTypes: true })
        .filter((e) => e.isDirectory() && !SKIP_DIRS.has(e.name))
        .map((e) => e.name)
    } catch {
      violations.push(`C-3 ${end}/wasm-apps 不存在 —— 覆盖面静默归零，本锁必须报错`)
      continue
    }
    const withSrc = apps.filter((app) => io.existsDir(join(appsRoot, app, 'src')))
    if (withSrc.length === 0) {
      violations.push(`C-3 ${end}/wasm-apps 下没有任何含 \`src/\` 的应用 —— 扫描面空转，本锁失效`)
    }
    for (const app of withSrc) {
      const srcRel = `${end}/wasm-apps/${app}/src`
      const files = io.filesUnder(join(REPO_ROOT, srcRel))
      const covered = matchers.some(({ re }) => re && files.some((f) => re.test(`${srcRel}/${f}`)))
      if (!covered) {
        violations.push(`C-3 应用未被任何 glob 覆盖（其前端零资源访问锁不生效）：\`${srcRel}/\``)
      }
    }
  }

  return violations
}

/** 真实 fs 适配器 */
const realIo = {
  existsDir: isDir,
  filesUnder: (dir) => listFiles(dir),
}

function loadRealGlobs() {
  return extractGlobs(readFileSync(join(REPO_ROOT, CONFIG_REL), 'utf8'))
}

describe('eslint.config.js frontendSourceGlobs ↔ 真实目录', () => {
  it('正例（C-1/C-2/C-3）：真实配置零违规', () => {
    const violations = validateGlobs(loadRealGlobs(), realIo)
    assert.deepEqual(violations, [], `真实 eslint.config.js 的 glob 与目录不一致：\n  - ${violations.join('\n  - ')}`)
  })

  it('反例（C-1/C-3）：目录改名后的旧 glob（历史形态）必须报违规', () => {
    const globs = loadRealGlobs().map((g) => g.replace('bedcode-mobile/wasm-apps/', 'bedcode-mobile/plugins/'))
    const violations = validateGlobs(globs, realIo)
    assert.ok(
      violations.some((v) => v.includes('C-1') && v.includes('bedcode-mobile/plugins')),
      `旧的 \`plugins/\` glob 未被 C-1 抓到（这正是 2026-10-16 静默失效的形态）：${violations.join(' | ')}`,
    )
    assert.ok(
      violations.some((v) => v.includes('C-3') && v.includes('bedcode-mobile/wasm-apps/')),
      `定义端覆盖缺失未被 C-3 抓到：${violations.join(' | ')}`,
    )
  })

  it('反例（C-3）：删掉移动端 wasm-apps glob → 逐一相关应用被点名', () => {
    const globs = loadRealGlobs().filter((g) => !g.startsWith('bedcode-mobile/wasm-apps/'))
    const violations = validateGlobs(globs, realIo)
    const mobileApps = readdirSync(join(REPO_ROOT, 'bedcode-mobile', 'wasm-apps'), { withFileTypes: true })
      .filter((e) => e.isDirectory() && isDir(join(REPO_ROOT, 'bedcode-mobile', 'wasm-apps', e.name, 'src')))
      .map((e) => e.name)
    assert.ok(mobileApps.length > 0, '前置失效：移动端一个带 src 的应用都没有')
    for (const app of mobileApps) {
      assert.ok(
        violations.some((v) => v.includes(`bedcode-mobile/wasm-apps/${app}/src`)),
        `删掉移动 glob 后未点名应用 ${app}：${violations.join(' | ')}`,
      )
    }
  })

  it('反例（C-1/C-2）：指向不存在目录 / 空目录的 glob 必须报违规', () => {
    const ghost = 'bedcode-desktop/not-a-real-dir/**/*.ts'
    const violations = validateGlobs([...loadRealGlobs(), ghost], realIo)
    assert.ok(
      violations.some((v) => v.includes('C-1') && v.includes(ghost)),
      `指向不存在目录的 glob 未被抓到：${violations.join(' | ')}`,
    )
  })

  it('异常（C-4）：抽取不到数组 / 数组为空 ⇒ 抛错（不许静默通过）', () => {
    assert.throws(() => extractGlobs('const other = []\n'), /抽取不到/, '数组改名后必须抛错')
    assert.throws(() => extractGlobs('const frontendSourceGlobs = [\n]\n'), /为空/, '空清单必须抛错')
    assert.throws(() => validateGlobs([], realIo), /为空/, '空清单必须在校验器边界抛错')
  })

  it('正例（匹配器自证）：`**/` 跨目录与 `{a,b}` 展开都真的生效', () => {
    const re = globToRegExp('bedcode-mobile/wasm-apps/*/src/**/*.{ts,vue}')
    assert.ok(re.test('bedcode-mobile/wasm-apps/bedcode/src/x.ts'), '同目录单层文件应命中')
    assert.ok(re.test('bedcode-mobile/wasm-apps/bedcode/src/a/b/Deep.vue'), '`**/` 深层文件应命中')
    assert.ok(!re.test('bedcode-mobile/wasm-apps/bedcode/lib/x.ts'), '非 src/ 下的文件不应命中')
    assert.ok(!re.test('bedcode-mobile/wasm-apps/a/b/src/x.ts'), '`*` 不得跨目录段')
    assert.ok(!re.test('bedcode-mobile/wasm-apps/bedcode/src/x.css'), '扩展名集合外的文件不应命中')
  })
})
