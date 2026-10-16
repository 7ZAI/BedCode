/**
 * 防泄漏回归防线 ②（票 04 / ADR 0030）：i18n 键无 `{error}` 插值残留扫描测试。
 *
 * 背景：票 01-04 清剿了 12 组带 `{error}` 插值的 i18n key（技术详情经占位符直显
 * 的旧模式）。此测试把「新代码不得再引入 `{error}`」从依赖纪律升级为**机器防线**：
 * 一旦有人给 message 值塞回 `{error}` 占位符，CI 即红。
 *
 * 扫描对象：宿主全部 locale（zh-CN + en × common/desktop/settings）的所有消息值。
 * clean 规则：
 * - 值中含 `{error}`（误插值模式）→ fail
 * - 值中含 `{name}` / `{plugin}` / `{count}` 等**合法具名参数**→ pass（参数插值允许，
 *   只禁 `{error}` 与任何可能承载技术详情的占位符）
 * - 值中的 `{`、`}` 必须配对成单层具名占位符（结构性检查：不成对 = 拼写错误）
 */
import { describe, it, expect } from 'vitest'
import zhCommon from '@/locales/zh-CN/common'
import zhDesktop from '@/locales/zh-CN/desktop'
import zhSettings from '@/locales/zh-CN/settings'
import enCommon from '@/locales/en/common'
import enDesktop from '@/locales/en/desktop'
import enSettings from '@/locales/en/settings'
// 全仓扫描（票 04 遗留清扫）：wasm-apps 四个插件工程的 locale 同样受 {error} 禁令
import tsZh from '../../../wasm-apps/terminal-session/src/i18n/zh-CN'
import tsEn from '../../../wasm-apps/terminal-session/src/i18n/en'
import ahZh from '../../../wasm-apps/agent-hub/src/i18n/zh-CN'
import ahEn from '../../../wasm-apps/agent-hub/src/i18n/en'
import acZh from '../../../wasm-apps/ai-chatbox/src/i18n/zh-CN'
import acEn from '../../../wasm-apps/ai-chatbox/src/i18n/en'
import ftZh from '../../../wasm-apps/file-transfer/src/i18n/zh-CN'
import ftEn from '../../../wasm-apps/file-transfer/src/i18n/en'

/** 全部 locale 消息树：文件 → 默认导出对象 */
const LOCALE_TREES: Array<{ file: string; tree: Record<string, unknown> }> = [
  { file: 'zh-CN/common.ts', tree: zhCommon },
  { file: 'zh-CN/desktop.ts', tree: zhDesktop },
  { file: 'zh-CN/settings.ts', tree: zhSettings },
  { file: 'en/common.ts', tree: enCommon },
  { file: 'en/desktop.ts', tree: enDesktop },
  { file: 'en/settings.ts', tree: enSettings },
  // 插件工程（终端会话 / Agent Hub / AI Chatbox / 文件传输）
  { file: 'terminal-session/zh-CN.ts', tree: tsZh },
  { file: 'terminal-session/en.ts', tree: tsEn },
  { file: 'agent-hub/zh-CN.ts', tree: ahZh },
  { file: 'agent-hub/en.ts', tree: ahEn },
  { file: 'ai-chatbox/zh-CN.ts', tree: acZh },
  { file: 'ai-chatbox/en.ts', tree: acEn },
  { file: 'file-transfer/zh-CN.ts', tree: ftZh },
  { file: 'file-transfer/en.ts', tree: ftEn },
]

/**
 * 深度遍历消息树，收集「叶子字符串值 + key 路径」。
 * 函数值（vue-i18n 消息函数 / 运行时扩展）跳过。
 */
function collectLeafStrings(
  node: unknown,
  path: string,
  out: Array<{ path: string; value: string }>,
): void {
  if (typeof node === 'string') {
    out.push({ path, value: node })
  } else if (Array.isArray(node)) {
    node.forEach((item, i) => collectLeafStrings(item, `${path}[${i}]`, out))
  } else if (typeof node === 'object' && node !== null) {
    for (const [key, value] of Object.entries(node)) {
      collectLeafStrings(value, path ? `${path}.${key}` : key, out)
    }
  }
}

describe('防泄漏回归防线 ②：i18n 无 {error} 插值残留', () => {
  for (const { file, tree } of LOCALE_TREES) {
    describe(file, () => {
      const leaves: Array<{ path: string; value: string }> = []
      collectLeafStrings(tree, '', leaves)

      it('消息值不含 {error} 占位符', () => {
        const offenders = leaves.filter((l) => l.value.includes('{error}'))
        expect(offenders).toEqual([])
      })

      it('占位符必须成对且为合法具名参数（{name}/{plugin}/{count} 族）', () => {
        // 允许的具名参数白名单：!{...} 之外的任意小写标识符（数字/单词/连字符）。
        // 规则：值中每个 { 都必须闭合；非法形态（{error}、{unclosed、{0}、双嵌套）→ fail
        const badPlaceholders: Array<{ path: string; value: string; found: string }> = []
        for (const leaf of leaves) {
          const openCount = (leaf.value.match(/\{/g) ?? []).length
          const closeCount = (leaf.value.match(/\}/g) ?? []).length
          if (openCount !== closeCount) {
            badPlaceholders.push({
              path: leaf.path,
              value: leaf.value,
              found: `unbalanced braces {${openCount}}/${closeCount}`,
            })
            continue
          }
          // 单层具名占位符：{name} / {pluginId} / {viewId} ……；拒绝 {error}、空 {}、
          // 数字索引 {0}（vue-i18n 链表位）与嵌套形态
          const placeholder = /^\{(?!error\})[a-zA-Z][a-zA-Z0-9_-]*\}$/
          for (const match of leaf.value.matchAll(/\{[^{}]*\}/g)) {
            if (!placeholder.test(match[0])) {
              badPlaceholders.push({
                path: leaf.path,
                value: leaf.value,
                found: match[0],
              })
            }
          }
        }
        expect(badPlaceholders).toEqual([])
      })
    })
  }
})