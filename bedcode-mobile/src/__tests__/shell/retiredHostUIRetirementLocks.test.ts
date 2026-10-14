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
 *   R4 票 2026-10-10 批次 C1/C2/C4 退役面：旧嵌入扩展点（registerToolboxPage /
 *      registerNavTab / registerTerminalToolbarItem / registerTerminalView）及其
 *      宿主 UI 组件（PluginNavTabHost / PluginSettingsHost / PluginTerminalBar）与
 *      `mobile-settings-notifications` 路由不得回接；运行面解析不得再有回退链。
 *      —— 锁的是**调用面**而非单个文件：只要 src/ 里再出现任一退役符号即红。
 *   R5 票 2026-10-10 批次 D1 删除的孤儿组件 / composable 不得被重新接回宿主。
 *
 * 扫描一律跳过注释行：注释里出现退役字面量（如「旧宿主 MobileLayout 退役后……」）不应
 * 判违规——否则说明文字自身会触发红，还会诱导删注释。
 *
 * 例外（票 2026-10-10 C2）：`src/plugin/context.ts` 保留了四个退役扩展点的**显性抛错桩**
 * （§5.1.3 fail-visible 形态①，旧插件调用时当场指名报错而不是 undefined 调用崩在插件里）。
 * 那是退役面的一部分而非回接，故按文件白名单排除——排除名单是显式的，
 * 新增文件不会自动被放过。
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
/**
 * 允许包含退役符号的文件（逐条登记，不做通配）
 *
 * `src/plugin/context.ts`：C2 的 fail-visible 抛错桩，见本文件头注「例外」。
 * `src/__tests__/plugin/pluginContextShell.test.ts`：钉死抛错桩行为的用例。
 * `src/__tests__/shell/shellSettingsScreen.test.ts`：钉死「通知入口不得回接」的反向断言。
 */
const R4_ALLOWED_FILES = [
  'src/plugin/context.ts',
  'src/__tests__/plugin/pluginContextShell.test.ts',
  'src/__tests__/shell/shellSettingsScreen.test.ts',
]

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

/** 票 2026-10-10 C1/C2/C4 退役的旧嵌入扩展点与其宿主 UI 组件 */
const RETIRED_EXTENSION_POINTS = [
  'registerToolboxPage',
  'registerNavTab',
  'registerTerminalToolbarItem',
  'registerTerminalView',
  'PluginNavTabHost',
  'PluginSettingsHost',
  'PluginTerminalBar',
]

/** 票 2026-10-10 C4 退役的设置路由（整页皆业务项，已下沉 terminal-session） */
const RETIRED_C4_ROUTES = ['mobile-settings-notifications', 'NotificationSettingsView']

/** 票 2026-10-10 D1 从宿主删除的孤儿（各自在 wasm-apps 下有自持副本） */
const RETIRED_ORPHANS = [
  'DeviceCard',
  'SessionListItem',
  'useAndroidFeatures',
  'useRunTime',
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

describe('R4 旧嵌入扩展点与其宿主 UI 组件不回接（票 2026-10-10 C1/C2）', () => {
  it('should_notContainRetiredExtensionPoints_when_srcScanned', () => {
    const files = SRC_CODE_FILES.filter((f) => !R4_ALLOWED_FILES.includes(f))
    const hits = scan(new RegExp(`\\b(${RETIRED_EXTENSION_POINTS.join('|')})\\b`), files)
    expect(hits).toEqual([])
  })

  it('should_keepOnlySurfaceFallback_when_run', () => {
    // 运行面解析的正路：只认 registerSurface。回退链是「壳用旧宿主插件形式加载页面」
    // 的实际代码，删掉后这里钉住它不再以任何形式回来。
    const adapter = readFileSync(join(process.cwd(), 'src/shell/adapters/pluginAppSource.ts'), 'utf8')
    expect(stripComments(adapter)).not.toMatch(/toolboxViews|navTabs|terminalView\b/)
  })
})

describe('R4b 已下沉业务设置路由不回接（票 2026-10-10 C4）', () => {
  it('should_notContainRetiredC4SettingsRoutes_when_srcScanned', () => {
    const files = SRC_CODE_FILES.filter((f) => !R4_ALLOWED_FILES.includes(f))
    const hits = scan(new RegExp(`(${RETIRED_C4_ROUTES.join('|')})`), files)
    expect(hits).toEqual([])
  })
})

describe('R5 已删除的宿主孤儿组件不得回接（票 2026-10-10 D1）', () => {
  it('should_notContainRetiredOrphans_when_srcScanned', () => {
    const hits = scan(new RegExp(`\\b(${RETIRED_ORPHANS.join('|')})\\b`), SRC_CODE_FILES)
    expect(hits).toEqual([])
  })

  it('should_notExistOnDisk_when_orphansRetired', () => {
    for (const file of [
      'src/components/DeviceCard.vue',
      'src/components/SessionListItem.vue',
      'src/composables/useAndroidFeatures.ts',
      'src/composables/useRunTime.ts',
    ]) {
      expect(existsSync(join(process.cwd(), file)), `${file} 应已删除`).toBe(false)
    }
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
