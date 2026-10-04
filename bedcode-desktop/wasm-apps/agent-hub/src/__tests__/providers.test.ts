/**
 * 供应商域纯函数单测（票据 05）：内置模板形状 + notes 来源解析
 */
import { describe, expect, it } from 'vitest'
import {
  APPLY_TARGETS,
  defaultModelsUrl,
  deriveEnvKeyName,
  mergeModelIds,
  modelsToText,
  parseModelsText,
  PROVIDER_TEMPLATES,
  sourceFromNotes,
} from '../utils/providers'

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
  it('应用目标白名单：claude/pi/opencode/codex（codex = 登记 provider + 设为当前模型）', () => {
    expect(APPLY_TARGETS).toEqual(['claude', 'pi', 'opencode', 'codex'])
  })
})

describe('deriveEnvKeyName', () => {
  it('由预设名派生 `<NAME>_API_KEY`（codex env_key 的惯例形态）', () => {
    expect(deriveEnvKeyName('InkStone')).toBe('INKSTONE_API_KEY')
    expect(deriveEnvKeyName('opencode-go')).toBe('OPENCODE_GO_API_KEY')
    // 全非 ASCII 时不给假变量名（只留通用占位）
    expect(deriveEnvKeyName('商汤日日新')).toBe('API_KEY')
  })

  it('非法字符一律换成下划线，且不产生以数字开头的变量名（codex/guest 会拒）', () => {
    expect(deriveEnvKeyName('a.b-c d')).toBe('A_B_C_D_API_KEY')
    expect(deriveEnvKeyName('  spaced  ')).toBe('SPACED_API_KEY')
    expect(deriveEnvKeyName('___')).toBe('API_KEY')
    expect(deriveEnvKeyName('')).toBe('API_KEY')
    // 派生态必须始终是合法 shell 变量名
    for (const name of ['InkStone', '1abc', 'a b', '', '—', 'x'.repeat(50)]) {
      expect(deriveEnvKeyName(name)).toMatch(/^[A-Za-z_][A-Za-z0-9_]*$/)
    }
  })
})

describe('defaultModelsUrl', () => {
  it('由 baseUrl 派生 `{base}/models`，去尾斜杠', () => {
    expect(defaultModelsUrl('https://api.deepseek.com/v1')).toBe('https://api.deepseek.com/v1/models')
    expect(defaultModelsUrl('https://api.deepseek.com/v1/')).toBe(
      'https://api.deepseek.com/v1/models',
    )
    expect(defaultModelsUrl('  https://a.b/v1  ')).toBe('https://a.b/v1/models')
  })

  it('已以 /models 结尾时原样返回（不叠成 /models/models）', () => {
    expect(defaultModelsUrl('https://a.b/v1/models')).toBe('https://a.b/v1/models')
  })

  it('baseUrl 为空 → 空串（不产生 `/models` 这种无主 URL）', () => {
    expect(defaultModelsUrl('')).toBe('')
    expect(defaultModelsUrl('   ')).toBe('')
  })
})

describe('parseModelsText / modelsToText', () => {
  it('按行拆分、去空白行、去重保序（与 guest 侧 trim/filter 同语义）', () => {
    expect(parseModelsText('a\n\n  b  \na\nc')).toEqual(['a', 'b', 'c'])
    expect(parseModelsText('')).toEqual([])
    expect(parseModelsText('\n \n')).toEqual([])
  })

  it('往返：文本 → 列表 → 文本不产生尾随空行', () => {
    expect(modelsToText(parseModelsText('a\nb\n'))).toBe('a\nb')
    expect(modelsToText([])).toBe('')
  })
})

describe('mergeModelIds', () => {
  it('已有在前（手工顺序不变），新查询到的追加在后', () => {
    expect(mergeModelIds(['m1', 'm2'], ['m2', 'm3'])).toEqual(['m1', 'm2', 'm3'])
  })

  it('空白条目丢弃、不产生重复项', () => {
    expect(mergeModelIds(['m1'], ['  ', 'm1', ' m2 '])).toEqual(['m1', 'm2'])
    expect(mergeModelIds([], [])).toEqual([])
  })
})
