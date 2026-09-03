/**
 * i18n 同构断言：zh-CN 与 en 必须与 messages.ts schema 完全同构
 *
 * 编译期已由 `const zhCN: MessageSchema` / `const en: MessageSchema` 保证两语言文件
 * 均实现 schema（缺 key 即编译失败）；本测试在运行时深比较两语言文件的结构，
 * 捕获编译期类型无法暴露的漂移（key 遗漏/多余、叶子非字符串、空文案）。
 */
import { describe, it, expect } from 'vitest'
import zhCN from '../i18n/zh-CN'
import en from '../i18n/en'
import { messages } from '../i18n'

interface Leaf {
  key: string
  value: string
}

/** 深度遍历：叶子路径（'panel.title'）+ 叶子值 */
function collectLeaves(value: unknown, prefix = ''): Leaf[] {
  if (value === null || typeof value !== 'object') {
    return prefix ? [{ key: prefix, value: String(value) }] : []
  }
  const leaves: Leaf[] = []
  for (const [k, v] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${k}` : k
    if (v !== null && typeof v === 'object') {
      leaves.push(...collectLeaves(v, path))
    } else {
      leaves.push({ key: path, value: String(v) })
    }
  }
  return leaves
}

const zhLeaves = collectLeaves(zhCN)
const enLeaves = collectLeaves(en)

describe('i18n 同构（zh-CN / en）', () => {
  it('messages 导出仅含两个 locale（与宿主 vue-i18n 配置一致）', () => {
    expect(Object.keys(messages).sort()).toEqual(['en', 'zh-CN'])
  })

  it('zh-CN 与 en 的 key 集合完全一致（顺序无关）', () => {
    expect(zhLeaves.map((l) => l.key).sort()).toEqual(enLeaves.map((l) => l.key).sort())
  })

  it('叶子值均为非空字符串（无空文案占位）', () => {
    for (const leaf of [...zhLeaves, ...enLeaves]) {
      expect(leaf.value.trim(), `${leaf.key} 文案为空`).not.toBe('')
    }
  })

  it('面板关键文案均存在且两语言都有值', () => {
    for (const key of [
      'panel.title',
      'panel.empty',
      'panel.loadFailed',
      'panel.statusSucceeded',
      'panel.statusFailed',
      'panel.statusTimeout',
      'panel.statusMissed',
      'panel.statusWaiting',
      'panel.statusRunning',
    ]) {
      expect(zhLeaves.find((l) => l.key === key)?.value, `zh-CN 缺少 ${key}`).toBeTruthy()
      expect(enLeaves.find((l) => l.key === key)?.value, `en 缺少 ${key}`).toBeTruthy()
    }
  })
})
