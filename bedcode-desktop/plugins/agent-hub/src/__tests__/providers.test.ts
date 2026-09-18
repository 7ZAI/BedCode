/**
 * 供应商域纯函数单测（票据 05）：内置模板形状 + notes 来源解析
 */
import { describe, expect, it } from 'vitest'
import { APPLY_TARGETS, PROVIDER_TEMPLATES, sourceFromNotes } from '../utils/providers'

describe('PROVIDER_TEMPLATES', () => {
  it('复用 chatbox 四套模板 + custom 空白起点', () => {
    expect(PROVIDER_TEMPLATES.map((t) => t.id)).toEqual([
      'deepseek',
      'qwen',
      'openai',
      'anthropic',
      'custom',
    ])
  })

  it('四套模板 baseUrl 与模型列表非空；custom 为空', () => {
    for (const t of PROVIDER_TEMPLATES.filter((x) => x.id !== 'custom')) {
      expect(t.baseUrl.startsWith('https://')).toBe(true)
      expect(t.models.length).toBeGreaterThan(0)
      expect(t.name.length).toBeGreaterThan(0)
    }
    const custom = PROVIDER_TEMPLATES.find((t) => t.id === 'custom')!
    expect(custom.baseUrl).toBe('')
    expect(custom.models).toEqual([])
  })
})

describe('sourceFromNotes', () => {
  it('解析 pi/opencode/claude 来源标注', () => {
    expect(sourceFromNotes('pi:sensenova')).toEqual({ cli: 'pi', provider: 'sensenova' })
    expect(sourceFromNotes('opencode:gmi')).toEqual({ cli: 'opencode', provider: 'gmi' })
  })

  it('非来源标注返回 null（手工预设 notes 为空或非 `cli:provider` 形态）', () => {
    expect(sourceFromNotes(null)).toBeNull()
    expect(sourceFromNotes(undefined)).toBeNull()
    expect(sourceFromNotes('')).toBeNull()
    expect(sourceFromNotes('no-colon')).toBeNull()
    expect(sourceFromNotes('pi:')).toBeNull()
    expect(sourceFromNotes(':sensenova')).toBeNull()
    expect(sourceFromNotes('unknown-cli:provider')).toBeNull()
  })
})

describe('APPLY_TARGETS', () => {
  it('v1 应用目标白名单：claude/pi/opencode（codex 待格式校准）', () => {
    expect(APPLY_TARGETS).toEqual(['claude', 'pi', 'opencode'])
  })
})
