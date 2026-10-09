/**
 * 旧宿主 UI 退役面防回接锁（票 2026-10-09 阶段 B）
 * -----------------------------------------------------------------------------
 * 目的：旧前端宿主（MobileSwipeContainer 四页 + /mobile/** views + 旧嵌入面）退役后，
 * 钉死「不回接」——宿主 src/ 内不得再出现退役路由名 / 退役视图·组件符号。
 *
 * 与 ESLint 的分工：本锁是**测试期门禁**（`pnpm run test:run` 必跑），覆盖整个 src/
 * （含测试文件），不受 eslint ignores 影响，防止把退役面悄悄接回。
 *
 * 两把锁 + 一道正面钉：
 *   R1 退役路由名（mobile-devices / mobile-sessions / mobile-terminal / mobile-toolbox /
 *      mobile-preset-tasks / mobile-plugins / mobile-home-alt）不得以字符串字面量出现。
 *      —— 路由名带引号锁定，避免误伤 `--mobile-terminal-bg` 这类 CSS 变量子串。
 *   R2 退役视图 / 组件符号（MobileSwipeContainer / MobileNav / MobileLayout / MobileStatusBar /
 *      DevicesView / SessionsView / ToolboxView / PluginView / SettingsView / PresetTasksView /
 *      TerminalView / registerSettingsSection）不得以词边界符号出现。
 *      —— 词边界保证不误伤仍在服役的合法面：`PluginViewHost` / `EgressSettingsView` /
 *      `registerTerminalView` / `TerminalViewContribution` 等均不含词边界匹配。
 *   R3 壳等价物正面在场（ShellView / ShellHost / ShellTabbar / ShellSettingsScreen + 路由
 *      mobile-shell）——防「把新面也删掉」绕过退役锁（删空则 R1/R2 恒真）。
 *
 * 扫描一律跳过注释行：注释里出现退役字面量（如「旧宿主 MobileLayout 退役后……」）不应
 * 判违规——否则说明文字自身会触发红，还会诱导删注释。
 */
import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { join, relative } from 'node:path'

const SRC_DIR = join(process.cwd(), 'src')

/** 扫描到的源码文件（相对路径，POSIX 分隔符，便于断言信息可读） */
function collectFiles(dir: string, exts: string[]): string[] {
  if (!existsSync(dir)) return []
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name)
    if (entry.isDirectory()) {
      out.push(...collectFiles(full, exts))
    } else if (exts.some((e) => entry.name.endsWith(e))) {
      out.push(relative(process.cwd(), full).split('\\').join('/'))
    }
  }
  return out
}

/** 去掉块注释 / 行注释 / HTML 注释，返回「可执行文本」用于扫描 */
function stripComments(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    // 行注释：排除 `://`（协议）与 `"//"`（字符串里的双斜杠），只删真实注释
    .replace(/(^|[^:"'`])\/\/[^\n]*/g, '$1')
}

/** 违规详情：文件名 + 命中文本，失败信息能直接定位 */
function scan(pattern: RegExp, files: string[]): string[] {
  const hits: string[] = []
  for (const file of files) {
    const text = stripComments(readFileSync(join(process.cwd(), file), 'utf8'))
    const match = text.match(pattern)
    if (match) hits.push(`${file} → ${match[0].trim().slice(0, 80)}`)
  }
  return hits
}

// 排除锁文件自身：被锁符号清单的定义就含这些字面量，扫到自己会自指红。
const SRC_CODE_FILES = collectFiles(SRC_DIR, ['.ts', '.vue']).filter(
  (f) => !f.endsWith('retiredHostUIRetirementLocks.test.ts'),
)

/** 退役路由名（旧 vue-router 的 name）：带引号锁定，防误伤 CSS 变量子串 */
const RETIRED_ROUTE_NAMES = [
  'mobile-devices',
  'mobile-sessions',
  'mobile-terminal',
  'mobile-toolbox',
  'mobile-preset-tasks',
  'mobile-plugins',
  'mobile-home-alt',
]

/** 退役视图 / 组件符号：词边界锁定（`PluginViewHost` 等服役面不误伤） */
const RETIRED_SYMBOLS = [
  'MobileSwipeContainer',
  'MobileNav',
  'MobileLayout',
  'MobileStatusBar',
  'DevicesView',
  'SessionsView',
  'ToolboxView',
  'PluginView',
  'SettingsView',
  'PresetTasksView',
  'TerminalView',
  'registerSettingsSection',
]

/** 壳等价物：退役面的正面替代——缺件即红，防「删新面绕过退役锁」 */
const REQUIRED_SHELL_FILES = [
  'src/shell/views/ShellView.vue',
  'src/shell/components/ShellHost.vue',
  'src/shell/components/ShellTabbar.vue',
  'src/shell/components/screens/ShellSettingsScreen.vue',
]

describe('R1 退役路由名不回接', () => {
  it('should_haveSourceToScan_when_lockRuns', () => {
    // 守卫：目录为空 / 路径写错时本锁会全部恒真，先证明扫描集非空
    expect(SRC_CODE_FILES.length).toBeGreaterThan(80)
  })

  it('should_notContainRetiredRouteNames_when_srcScanned', () => {
    const pattern = new RegExp(`['"](${RETIRED_ROUTE_NAMES.join('|')})['"]`)
    const hits = scan(pattern, SRC_CODE_FILES)
    expect(hits).toEqual([])
  })
})

describe('R2 退役视图 / 组件符号不回接', () => {
  it('should_notContainRetiredSymbols_when_srcScanned', () => {
    const pattern = new RegExp(`\\b(${RETIRED_SYMBOLS.join('|')})\\b`)
    const hits = scan(pattern, SRC_CODE_FILES)
    expect(hits).toEqual([])
  })
})

describe('R3 壳等价物在场（正面钉）', () => {
  it('should_keepShellEquivalentsPresent_when_lockRuns', () => {
    for (const file of REQUIRED_SHELL_FILES) {
      expect(existsSync(join(process.cwd(), file)), `${file} 应存在（旧宿主的壳内等价面）`).toBe(
        true,
      )
    }
    const router = readFileSync(join(process.cwd(), 'src/router/index.ts'), 'utf8')
    expect(router, '壳默认路由 mobile-shell 应在 router 在场').toContain('mobile-shell')
  })
})
