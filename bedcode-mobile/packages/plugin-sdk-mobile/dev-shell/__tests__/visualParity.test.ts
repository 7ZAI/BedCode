/**
 * 视觉层漂移锁：dev-shell ↔ 移动端宿主（token / Tailwind 主题 / token 接线）
 * -----------------------------------------------------------------------------
 * `contractDrift.test.ts` 钉的是**类型契约**；这里钉的是**视觉真源**，因为两者
 * 漂移的后果不同：类型漂移表现为行为不一致，视觉漂移表现为「预览里另一套配色」。
 *
 * 真实事故（2026-10）：dev-shell 的 `mobile.css` 停在 2026-09-04，宿主 10-07 换了暖色
 * 「墨纸」体系，于是 file-transfer 在真机是暖色、在 dev-shell 是冷灰蓝——插件代码一行
 * 没动，预览却「长得不一样」。根因是 token 存在两份可各改各的副本，没有任何约束。
 *
 * 现在 token 走单一真源（monorepo 内 dev-shell 直接 import 宿主 mobile.css，见
 * dev-shell/vite.config.ts 的 `@bedcode/mobile-styles`），自带副本只服务 npm 包。
 * 于是漂移只剩一条路径：宿主改了而 npm 副本没跟——这正是本文件要钉住的东西。
 *
 * 宿主源码不在场时（npm 包内的 dev-shell）整组跳过：没有可比对象，跳过比假装通过诚实。
 */
import { describe, it, expect } from 'vitest'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
/** dev-shell/__tests__ → 仓库内移动端宿主：bedcode-mobile（__tests__→dev-shell→sdk→packages→mobile） */
const HOST_ROOT = resolve(here, '../../../..')
const HOST_MOBILE_CSS = resolve(HOST_ROOT, 'src/styles/mobile.css')
const LOCAL_MOBILE_CSS = resolve(here, '../src/styles/mobile.css')
const HOST_SHELL_CSS = resolve(HOST_ROOT, 'src/shell/styles/shell.css')
const LOCAL_SHELL_CSS = resolve(here, '../src/shell/styles/shell.css')
const HOST_TAILWIND = resolve(HOST_ROOT, 'tailwind.config.js')
const LOCAL_TAILWIND = resolve(here, '../tailwind.config.js')

const hostPresent = existsSync(HOST_MOBILE_CSS)
const describeIfHostPresent = hostPresent ? describe : describe.skip

/**
 * 按出现顺序抽出全部 `--mobile-*` 声明（key + value），**不按 key 归并**
 *
 * 归并成 Map 是错的：同一个 token 在深色段与 `html:not(.dark)` 浅色段各声明一次，
 * 归并后只剩浅色段的值，深色段整体换色（本次事故的形态）会整片漏检。
 * 声明序列逐条比，才能同时锁住「键变了」和「某个主题段的值变了」。
 */
function declarationsOf(source: string, prefix = '--mobile-'): Array<[string, string]> {
  const out: Array<[string, string]> = []
  for (const line of source.split('\n')) {
    const m = line.match(new RegExp(`^\\s*(${prefix}[A-Za-z0-9-]+):\\s*(.+?);\\s*$`))
    if (m) out.push([m[1], m[2].trim()])
  }
  return out
}

/** 首条不一致的声明（失败信息只报这一条，而不是丢一整页 diff） */
function firstMismatch(host: Array<[string, string]>, local: Array<[string, string]>) {
  const len = Math.min(host.length, local.length)
  for (let i = 0; i < len; i += 1) {
    if (host[i][0] !== local[i][0]) {
      return `第 ${i + 1} 条声明的 token 名不同：宿主 ${host[i][0]} / 副本 ${local[i][0]}`
    }
    if (host[i][1] !== local[i][1]) {
      return `第 ${i + 1} 条 ${host[i][0]} 取值不同：宿主 ${host[i][1]} / 副本 ${local[i][1]}`
    }
  }
  if (host.length !== local.length) {
    const longer = host.length > local.length ? host : local
    return `声明条数不同：宿主 ${host.length} 条 / 副本 ${local.length} 条，副本缺 ${longer[len][0]}`
  }
  return null
}

describeIfHostPresent('设计 token 与宿主一致', () => {
  it('should_declareSameTokenKeys_when_BundledCopyCompared', () => {
    // 键集比对：宿主新增 token 而副本没跟时，预览里 var(--mobile-xxx) 直接解析失败
    // 回退到继承色/透明——这类漂移肉眼表现为「某块颜色莫名不对」，比整体换色更难查
    const host = declarationsOf(readFileSync(HOST_MOBILE_CSS, 'utf-8')).map(([k]) => [k, ''] as [string, string])
    const local = declarationsOf(readFileSync(LOCAL_MOBILE_CSS, 'utf-8')).map(([k]) => [k, ''] as [string, string])
    expect(firstMismatch(host, local)).toBeNull()
  })

  it('should_declareSameTokenValues_when_BundledCopyCompared', () => {
    // 取值比对才是 2026-10 那次事故的直接门禁：键集完全一致，深色段的值却差一片。
    // 断言「逐条声明」而不是「每个 key 的最终值」——后者看不见深浅两段里的任意一段。
    const host = declarationsOf(readFileSync(HOST_MOBILE_CSS, 'utf-8'))
    const local = declarationsOf(readFileSync(LOCAL_MOBILE_CSS, 'utf-8'))
    expect(firstMismatch(host, local)).toBeNull()
  })

  it('should_matchHostSource_when_ShellCssCompared', () => {
    // 壳的样式表是纯副本（无 mock 分支），逐字一致即可；分叉同样是预览/真机两套观感
    expect(readFileSync(LOCAL_SHELL_CSS, 'utf-8')).toBe(readFileSync(HOST_SHELL_CSS, 'utf-8'))
  })
})

describeIfHostPresent('Tailwind 主题与宿主一致', () => {
  it('should_matchHostTheme_when_ConfigCompared', async () => {
    // 深比对 theme：调色板少一项 = 插件用到该类时被 purge 成无样式。
    // content 故意不比（dev-shell 要额外扫被调试插件目录），故只锁 theme。
    const host = (await import(/* @vite-ignore */ HOST_TAILWIND)).default
    const local = (await import(/* @vite-ignore */ LOCAL_TAILWIND)).default
    expect(local.theme).toEqual(host.theme)
    expect(local.darkMode).toEqual(host.darkMode)
  })
})

describeIfHostPresent('token 接线指向宿主真源', () => {
  it('should_resolveToHostStyles_when_TokenAliasResolved', async () => {
    // 单源接线的行为契约：monorepo 内 alias 必须落在宿主 src/styles/，
    // 否则「副本跟上了」也会被误当成「预览用的是宿主配色」——这是本组锁的前提
    const { default: config } = await import(/* @vite-ignore */ resolve(here, '../vite.config.ts'))
    const resolved = config({ command: 'serve', mode: 'test' }) as {
      resolve: { alias: Record<string, string> }
    }
    expect(resolved.resolve.alias['@bedcode/mobile-styles']).toBe(resolve(HOST_ROOT, 'src/styles'))
  })

  it('should_importTokenAlias_when_EntryImported', () => {
    // 入口必须走 alias 而不是相对路径 './styles/mobile.css'：
    // 相对路径会绕过接线、悄悄回到副本上（正是本次要消灭的漂移路径）
    const entry = readFileSync(resolve(here, '../src/main.ts'), 'utf-8')
    expect(entry).toContain("import '@bedcode/mobile-styles/mobile.css'")
    expect(entry).not.toContain("from './styles/mobile.css'")
  })
})