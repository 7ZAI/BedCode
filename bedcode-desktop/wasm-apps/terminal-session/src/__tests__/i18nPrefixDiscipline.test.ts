/**
 * 插件 i18n 前缀纪律锁（2026-09-26 立）
 *
 * 背景：插件文案经 `context.i18n.registerMessages` 注册时**自动加插件 ID 前缀**
 * （实际 key = `com.bedcode.terminal-session.session.terminal.settings` 等，宿主命名空间
 * 隔离机制）。插件组件取文案必须经 `context.i18n.t`（内部自动补前缀）。
 *
 * 若组件直接用 vue-i18n 的 `useI18n()`：`t('session.terminal.settings')` 会在**宿主**
 * 命名空间下查无此 key（宿主只留 `desktop.terminal.*`），vue-i18n 找不到即原样回显 key
 * —— 界面出现 `session.terminal.settings` 这类「key 原文」而不是中文（2026-09-26 终端
 * 独立窗口标题栏实况）。本锁把「插件前端不得直接 import vue-i18n」变成门禁。
 *
 * 允许：`context.i18n.t` / `context.i18n.getI18n()`（取 locale 等只读用途）。
 * 禁止：`from 'vue-i18n'` / `import('vue-i18n')`（含 useI18n 与其它无前缀 API）。
 */

import { describe, it, expect } from 'vitest'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** 插件前端源码根（vitest 工作目录为 bedcode-desktop，同 tailwindContentCoverage 用例口径） */
const PLUGIN_SRC = join('wasm-apps', 'terminal-session', 'src')

/** 判定：源码文本是否直接引入 vue-i18n（注释里的提及不算——只认 import 语法） */
function hasDirectI18nImport(text: string): boolean {
  return /from\s*['"]vue-i18n['"]|import\s*\(\s*['"]vue-i18n['"]\s*\)|require\s*\(\s*['"]vue-i18n['"]\s*\)/.test(
    text,
  )
}

/** 递归收集插件前端源码（排除测试目录；判据只面向 .ts / .vue 交付源码） */
function collectSourceFiles(dir: string): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === '__tests__' || entry.name === 'node_modules' || entry.name === 'dist') {
      continue
    }
    const full = join(dir, entry.name)
    if (entry.isDirectory()) {
      out.push(...collectSourceFiles(full))
    } else if (/\.(ts|vue)$/.test(entry.name)) {
      out.push(full)
    }
  }
  return out
}

describe('插件 i18n 前缀纪律锁', () => {
  it('正例：直接 import vue-i18n 被判定为违规', () => {
    expect(hasDirectI18nImport(`import { useI18n } from 'vue-i18n'`)).toBe(true)
    expect(hasDirectI18nImport(`const m = await import('vue-i18n')`)).toBe(true)
  })

  it('反例：经 context.i18n 取文案 / 注释提及 vue-i18n 不算违规', () => {
    expect(
      hasDirectI18nImport(
        `const t = (key: string) => context.i18n.t(key)\n// 不可用 vue-i18n 的 useI18n()（无前缀查不到文案）`,
      ),
    ).toBe(false)
    expect(hasDirectI18nImport(`const i18n = context.i18n.getI18n()`)).toBe(false)
  })

  it('集成：插件全部前端源码零直接引入 vue-i18n', () => {
    const files = collectSourceFiles(PLUGIN_SRC)

    // 防「扫描空转 = 全绿」：真实扫到了插件源码
    expect(files.length).toBeGreaterThanOrEqual(30)

    const offenders = files.filter((file) => hasDirectI18nImport(readFileSync(file, 'utf-8')))

    expect(
      offenders,
      `插件前端禁止直接引入 vue-i18n（无前缀 t 会回显 key 原文）；请改用 context.i18n.t：${offenders.join(', ')}`,
    ).toEqual([])
  })
})
