/**
 * 供应商图标解析单测（接缝 2）：resolveProviderIcon 映射、首字母取色确定性
 */
import { describe, it, expect } from 'vitest'
import { resolveProviderIcon, providerAvatarColor } from '../utils/providerIcons'

describe('resolveProviderIcon', () => {
  it('内置预设 id → 对应品牌图标（非空资源引用）', () => {
    expect(resolveProviderIcon('deepseek')).toBeTruthy()
    expect(resolveProviderIcon('qwen')).toBeTruthy()
    expect(resolveProviderIcon('openai')).toBeTruthy()
    expect(resolveProviderIcon('anthropic')).toBeTruthy()
  })

  it('各预设 id 指向不同图标资源', () => {
    const icons = ['deepseek', 'qwen', 'openai', 'anthropic'].map(id => resolveProviderIcon(id))
    expect(new Set(icons).size).toBe(4)
  })

  it('无 presetId / 未知 id → null（走首字母头像兜底）', () => {
    expect(resolveProviderIcon(undefined)).toBeNull()
    expect(resolveProviderIcon('')).toBeNull()
    expect(resolveProviderIcon('unknown-vendor')).toBeNull()
  })
})

describe('providerAvatarColor', () => {
  it('同名称必同色（确定性，供首字母头像展示）', () => {
    expect(providerAvatarColor('OpenRouter')).toBe(providerAvatarColor('OpenRouter'))
    expect(providerAvatarColor('本地网关')).toBe(providerAvatarColor('本地网关'))
    expect(providerAvatarColor('  My Proxy  ')).toBe(providerAvatarColor('My Proxy'))
  })

  it('不同名称允许同色（哈希碰撞），但颜色必须来自主题色板', () => {
    // 色板是固定 8 色集合，任何输入都只能落在其中
    const palette = ['#4f46e5', '#0ea5e9', '#059669', '#d97706', '#dc2626', '#7c3aed', '#db2777', '#0891b2']
    for (const name of ['a', 'b', 'c', '中文名', 'x'.repeat(30)]) {
      expect(palette).toContain(providerAvatarColor(name))
    }
  })

  it('空名称/纯空白回退色板首个色，不崩溃', () => {
    expect(providerAvatarColor('')).toBe('#4f46e5')
    expect(providerAvatarColor('   ')).toBe('#4f46e5')
  })
})
