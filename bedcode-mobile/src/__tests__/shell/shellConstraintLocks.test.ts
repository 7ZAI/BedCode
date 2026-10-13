/**
 * 宿主壳约束锁（源码扫描型）
 * -----------------------------------------------------------------------------
 * 目的：把移动端既有的前端限制机制在**壳层**钉死，防止「新目录 = 纪律空白」。
 *
 * 与 ESLint 的分工：ESLint 锁（`eslint.config.js` 的 frontendResourceAccessLock）
 * 已覆盖 `bedcode-mobile/src/**`（含 shell），是运行 lint 时的门禁；本文件是
 * **测试期门禁**——`pnpm run test:run` 必跑，且不受 lint 的 ignores 影响，
 * 防止有人改窄 ESLint 的 glob 后壳层悄悄失去约束。
 *
 * 七把锁：
 *   L1 前端零资源访问（AGENTS.md §6 前端红线）
 *   L2 不经裸 invoke（宿主调用只走既有命令封装 / 适配器）
 *   L3 平台授权弹窗单一挂载（重复挂载 = 双事件监听 + 双弹）
 *   L4 无静默 catch（错误必须进 logger / toast）
 *   L5 用户可见文案走 i18n（模板区不得出现中文）
 *   L6 token-bound（除 token 定义文件外不得硬编码颜色）
 *   L7 壳内自足：不依赖旧界面目录 + 迁移面不得缩水（组件库 / 机制副本在场）
 *
 * 锁索引同步登记在 `docs/code-map.md` 文末「防回接锁索引」——新增壳锁时一并更新，
 * 否则锁会无人知晓而漂移。
 *
 * 扫描一律跳过注释行：注释里出现被锁字面量不应判违规（否则会把说明文字
 * 当成违规，也会诱导人删注释）。
 */
import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { join, relative } from 'node:path'

const SHELL_DIR = join(process.cwd(), 'src/shell')
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

/** 取 .vue 的 <template> 区块（用于 i18n / 颜色锁） */
function templateBlocks(src: string): string[] {
  const blocks: string[] = []
  const re = /<template[^>]*>([\s\S]*?)<\/template>/g
  let m: RegExpExecArray | null
  while ((m = re.exec(src)) !== null) blocks.push(m[1])
  return blocks
}

/** 违规详情：文件名 + 命中文本，失败信息能直接定位 */
function scan(pattern: RegExp, files: string[], pick: (raw: string) => string): string[] {
  const hits: string[] = []
  for (const file of files) {
    const raw = readFileSync(join(process.cwd(), file), 'utf8')
    const text = pick(raw)
    const match = text.match(pattern)
    if (match) hits.push(`${file} → ${match[0].trim().slice(0, 80)}`)
  }
  return hits
}

const SHELL_CODE_FILES = collectFiles(SHELL_DIR, ['.ts', '.vue'])
const SHELL_VUE_FILES = SHELL_CODE_FILES.filter((f) => f.endsWith('.vue'))

describe('L1 前端零资源访问（壳层）', () => {
  it('should_haveShellSourcesToScan_when_lockRuns', () => {
    // 守卫：目录为空 / 路径写错时本锁会全部恒真，先证明扫描集非空
    expect(SHELL_CODE_FILES.length).toBeGreaterThan(20)
  })

  it('should_notUseNetworkOrFilePrimitives_when_shellSourceScanned', () => {
    const hits = scan(
      /\b(fetch\s*\(|XMLHttpRequest|WebSocket|EventSource|sendBeacon|__TAURI__)\b/,
      SHELL_CODE_FILES,
      stripComments,
    )
    expect(hits).toEqual([])
  })

  it('should_notImportCapabilityTauriPlugins_when_shellSourceScanned', () => {
    const hits = scan(
      /@tauri-apps\/plugin-(http|fs|shell|updater|opener)/,
      SHELL_CODE_FILES,
      stripComments,
    )
    expect(hits).toEqual([])
  })

  it('should_notUseNavigationPrimitives_when_shellSourceScanned', () => {
    const hits = scan(
      /(?:\b(?:window|self|globalThis)\.(?:open|assign|replace)\s*\(|\blocation\.(?:href|assign|replace)\s*[=(])/,
      SHELL_CODE_FILES,
      stripComments,
    )
    expect(hits).toEqual([])
  })
})

describe('L2 不经裸 invoke', () => {
  it('should_notImportInvokeDirectly_when_shellSourceScanned', () => {
    // 宿主调用一律经既有命令封装（@/plugin/commands）；壳内裸 invoke 会绕过
    // 既有封装里的错误处理与上下文
    const hits = scan(/from\s+['"]@tauri-apps\/api\/core['"]/, SHELL_CODE_FILES, stripComments)
    expect(hits).toEqual([])
  })
})

describe('L3 平台授权弹窗单一挂载', () => {
  const GLOBAL_DIALOGS = ['FsAuthDialog', 'EgressConsentDialog']

  it('should_notMountGlobalConsentDialogs_when_shellSourceScanned', () => {
    // 这两个弹窗已在 App.vue 全局挂载并各自监听 Rust 事件；壳内再挂一份
    // 会出现双监听、同一请求弹两次（回执也会发两次）
    for (const name of GLOBAL_DIALOGS) {
      const hits = scan(new RegExp(`\\b${name}\\b`), SHELL_CODE_FILES, (raw) => raw)
      expect(hits).toEqual([])
    }
  })

  it('should_mountEachConsentDialogExactlyOnce_when_appRootScanned', () => {
    const appVue = readFileSync(join(SRC_DIR, 'App.vue'), 'utf8')
    for (const name of GLOBAL_DIALOGS) {
      // 只数模板挂载标签 `<Name .../>`（import 绑定与路径字符串里出现的同名不计入），
      // 必须恰好一处；0 说明挂载丢了，>1 说明重复挂载（会双监听双弹）
      const mountCount = (appVue.match(new RegExp(`<${name}\\b`, 'g')) ?? []).length
      expect(mountCount, `${name} 在 App.vue 的模板挂载次数`).toBe(1)
      // 同时确认确有 import（防止挂载被静默删掉却仍留着标签）
      expect(appVue, `${name} 应在 App.vue 被 import`).toContain(`import ${name} `)
    }
  })
})

describe('L4 无静默 catch', () => {
  it('should_reportEveryCaughtError_when_shellSourceScanned', () => {
    const silent: string[] = []
    for (const file of SHELL_CODE_FILES) {
      const lines = readFileSync(join(process.cwd(), file), 'utf8').split(/\r?\n/)
      lines.forEach((line, i) => {
        if (!/\bcatch\b/.test(line)) return
        // catch 后的小块内必须出现 logger. 或 toast.（错误要可观测，不能吞掉）
        const window = lines.slice(i, i + 6).join('\n')
        if (!/\b(logger|toast)\s*\./.test(window)) silent.push(`${file}:${i + 1} → ${line.trim()}`)
      })
    }
    expect(silent).toEqual([])
  })
})

describe('L5 用户可见文案走 i18n', () => {
  it('should_notContainChineseInTemplates_when_vueFilesScanned', () => {
    const hits: string[] = []
    for (const file of SHELL_VUE_FILES) {
      const raw = readFileSync(join(process.cwd(), file), 'utf8')
      for (const block of templateBlocks(raw)) {
        const text = stripComments(block)
        // CJK 统一表意文字区（U+4E00–U+9FFF）：避开 \p{Han} 的 unicode 属性转义，
        // 该写法在本项目构建链路（esbuild/rollup 目标）下解析报错
        const match = text.match(/[一-鿿]/)
        if (match) hits.push(`${file} → ${match[0]}`)
      }
    }
    expect(hits).toEqual([])
  })
})

describe('L6 token-bound（无硬编码颜色）', () => {
  it('should_notHardcodeColors_when_shellSourcesScanned', () => {
    const hits = scan(
      /(#[0-9a-fA-F]{3,8}\b|\brgba?\s*\()/,
      // 排除 token 定义文件：品牌色与壳内结构 token 的唯一落点
      SHELL_CODE_FILES.filter((f) => !f.endsWith('styles/shell.css')),
      (raw) => stripComments(raw),
    )
    expect(hits).toEqual([])
  })
})

describe('L7 壳内自足：不依赖旧界面目录 + 迁移面不得缩水', () => {
  /**
   * 过渡期桥接白名单（当前为空 = 壳对旧公共组件 / 机制零依赖）。
   *
   * 规则（AGENTS.md §6「移动端前端重构优先对接宿主壳」）：壳内需要某个旧机制时
   * **先复制进 `src/shell/composables/**` 或 `src/shell/components/ui/**`**；确实无法
   * 复制（如需与旧线共用一个进程级单例）才允许桥接，并在本表登记具体模块路径 + 理由。
   */
  const BRIDGE_ALLOWLIST: string[] = []

  /** 公共组件库：迁移面清单——删文件即测红（防止用「删掉」绕过依赖锁） */
  const REQUIRED_UI_FILES = [
    'src/shell/components/ui/Button.vue',
    'src/shell/components/ui/Toggle.vue',
    'src/shell/components/ui/Modal.vue',
    'src/shell/components/ui/ConfirmDialog.vue',
    'src/shell/components/ui/PromptDialog.vue',
    'src/shell/components/ui/LoadingDialog.vue',
    'src/shell/components/ui/CollapseSection.vue',
    'src/shell/components/ui/QuickActionButton.vue',
    'src/shell/components/ui/LetterAvatar.vue',
  ]

  /** 机制副本：平台机制清单（业务机制不在此列——终端 / 文件 / 任务归各应用） */
  const REQUIRED_MECHANISM_FILES = [
    'src/shell/composables/useToast.ts',
    'src/shell/composables/usePlatform.ts',
    'src/shell/composables/useOrientation.ts',
    'src/shell/composables/useSwipeTabs.ts',
    'src/shell/composables/useViewportPanGuard.ts',
  ]

  it('should_notImportLegacyUiOrMechanismDirs_when_shellSourceScanned', () => {
    // 旧界面目录：components（公共组件）/ composables（机制）/ views（页面）。
    // 壳内出现这些 import 说明新界面又挂回旧真源——复制面会随旧目录退役而断链。
    const hits = scan(
      /from\s+['"]@\/(?:components|composables|views)\//,
      SHELL_CODE_FILES,
      stripComments,
    ).filter((hit) => !BRIDGE_ALLOWLIST.some((allow) => hit.includes(allow)))
    expect(hits).toEqual([])
  })

  it('should_keepPortedUiAndMechanismFilesPresent_when_lockRuns', () => {
    // 守卫：上面那把锁靠「不 import」判定，若把复制面删空则恒真——这里正面钉住在场
    for (const file of [...REQUIRED_UI_FILES, ...REQUIRED_MECHANISM_FILES]) {
      expect(existsSync(join(process.cwd(), file)), `${file} 应存在于壳内复制面`).toBe(true)
    }
  })
})
