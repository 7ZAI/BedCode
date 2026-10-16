/**
 * Tailwind content 覆盖锁（2026-09-23 立）
 *
 * 背景：插件前端的 Tailwind 工具类**只在宿主编译一次**（`tailwind.config.js` 的
 * `content` 逐插件列路径；宿主 vite / postcss 没有第二处注入点，插件产物也不携带
 * 编译后的 Tailwind）。因此 `content` 与 `wasm-apps/` 目录必须一一对应：
 * - 漏一条 → 该插件**独有**的工具类零产出（与宿主/其它插件重叠的类侥幸还在，
 *   表现为间距、固定宽高、栅格、状态色局部塌陷，不会整页崩，极易漏检）；
 * - 留一条死路径 → 指向已改名/已退役的目录，同样静默失效。
 *
 * 现场：2026-09-22 `session → terminal-session` 改名批次未同步本清单，`content` 里
 * 仍是 `./plugins/session/src/**`（目录已不存在）而缺 `terminal-session` 一条 ——
 * 实测 9 个 Vue / 77 处工具类引用拿不到 CSS（插件 405 个类令牌中 165 个仅本插件使用，
 * 其中 158 个未产出）。本用例把「改名/新增插件必须同步 content」变成门禁。
 */

import { describe, it, expect } from 'vitest'
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const TAILWIND_CONFIG = 'tailwind.config.js'
// 桌面端语义（2026-09-25）：wasm 插件对外称 wasm 应用，源码目录 wasm-apps/
const PLUGINS_DIR = 'wasm-apps'

/** 从 `tailwind.config.js` 文本抓出 `./wasm-apps/<id>/src/...` 形态的 content 条目（保序去重） */
function parseContentPluginDirs(configText: string): string[] {
  const dirs: string[] = []
  // id 段不允许出现 `*`：配置注释里引用了被否决的 `'./wasm-apps/**/src/**'` 通配写法，
  // 若把通配符当插件 id 会误报一条死路径（本用例首跑即由此转红）
  for (const match of configText.matchAll(/'(\.\/wasm-apps\/([^/'*]+)\/src\/[^']*)'/g)) {
    if (!dirs.includes(match[2])) dirs.push(match[2])
  }
  return dirs
}

/** 磁盘上真实存在的插件目录（判据：目录名即插件 id，且带 src/ 前端源码） */
function realPluginDirs(): string[] {
  return readdirSync(PLUGINS_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((name) => {
      const src = join(PLUGINS_DIR, name, 'src')
      return existsSync(src) && statSync(src).isDirectory()
    })
    .sort()
}

/** 双向缺口：内容清单漏掉的真实插件目录 / 指向不存在目录的死路径 */
function findGaps(
  pluginDirs: string[],
  contentDirs: string[],
): { missingInContent: string[]; deadContentPaths: string[] } {
  return {
    missingInContent: pluginDirs.filter((id) => !contentDirs.includes(id)).sort(),
    deadContentPaths: contentDirs.filter((id) => !pluginDirs.includes(id)).sort(),
  }
}

describe('Tailwind content 覆盖锁', () => {
  it('正例：目录与清单一一对应 → 双向零缺口', () => {
    const gaps = findGaps(['ai-chatbox', 'terminal-session'], ['ai-chatbox', 'terminal-session'])
    expect(gaps).toEqual({ missingInContent: [], deadContentPaths: [] })
  })

  it('反例：目录改名后未同步清单 → 报缺口（2026-09-22 session → terminal-session 现场）', () => {
    const gaps = findGaps(['session', 'terminal-session'], ['session'])
    expect(gaps.missingInContent).toEqual(['terminal-session'])
    expect(gaps.deadContentPaths).toEqual([])
  })

  it('反例：清单指向已退役插件 → 报死路径（scheduler 现场）', () => {
    const gaps = findGaps(['ai-chatbox'], ['ai-chatbox', 'scheduler'])
    expect(gaps.deadContentPaths).toEqual(['scheduler'])
    expect(gaps.missingInContent).toEqual([])
  })

  it('解析器认得真实配置的写法，且清单条目数有基线（防扫描空转全绿）', () => {
    const parsed = parseContentPluginDirs(readFileSync(TAILWIND_CONFIG, 'utf-8'))
    expect(parsed.length).toBeGreaterThanOrEqual(4)
    expect(parsed).toContain('terminal-session')
    expect(new Set(parsed).size).toBe(parsed.length)
  })

  it('集成：真实仓库的 plugins/ 与 content 双向零缺口', () => {
    const pluginDirs = realPluginDirs()
    const contentDirs = parseContentPluginDirs(readFileSync(TAILWIND_CONFIG, 'utf-8'))

    // 防「扫描空转 = 全绿」：磁盘上确实扫到了插件前端源码
    expect(pluginDirs.length).toBeGreaterThanOrEqual(4)

    const { missingInContent, deadContentPaths } = findGaps(pluginDirs, contentDirs)
    expect(
      missingInContent,
      `插件目录未纳入 tailwind content（该插件独有工具类将零产出）：${missingInContent.join(', ')}`,
    ).toEqual([])
    expect(
      deadContentPaths,
      `tailwind content 指向不存在的插件目录（死路径）：${deadContentPaths.join(', ')}`,
    ).toEqual([])
  })
})
