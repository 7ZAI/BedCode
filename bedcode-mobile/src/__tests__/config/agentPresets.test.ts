/**
 * agentPresets 命令预设单元测试
 *
 * 覆盖：会话启动命令 → Agent CLI 关键词包含匹配、各预设结构
 * （每套 12 条、5 类必选齐全、skills 位模式正确）。
 */
import { describe, it, expect } from 'vitest'
import { AGENT_PRESETS, AGENT_TYPES, detectAgentType, getPresetCommandTexts, filterPresetCommands } from '@/config/agentPresets'

describe('detectAgentType', () => {
  it('matches keywords by substring inclusion (case-insensitive)', () => {
    expect(detectAgentType('claude')).toBe('claude_code')
    expect(detectAgentType('npx claude code')).toBe('claude_code')
    expect(detectAgentType('claude.exe --dangerously-skip-permissions')).toBe('claude_code')
    expect(detectAgentType('codex')).toBe('codex')
    expect(detectAgentType('codex exec --full-auto')).toBe('codex')
    expect(detectAgentType('opencode')).toBe('opencode')
    expect(detectAgentType('npx opencode --config foo')).toBe('opencode')
    expect(detectAgentType('pi')).toBe('pi')
    expect(detectAgentType('npx pi -p "hello"')).toBe('pi')
  })

  it('returns generic when no keyword matches', () => {
    expect(detectAgentType('')).toBe('generic')
    expect(detectAgentType('npm run dev')).toBe('generic')
    expect(detectAgentType('powershell.exe')).toBe('generic')
  })
})

describe('AGENT_PRESETS', () => {
  it('covers all four agent types with 12 commands each', () => {
    expect(AGENT_TYPES).toHaveLength(4)
    for (const type of AGENT_TYPES) {
      const preset = AGENT_PRESETS[type]
      expect(preset).toHaveLength(12)
    }
  })

  it('each preset contains the 5 required categories', () => {
    for (const type of AGENT_TYPES) {
      const commands = AGENT_PRESETS[type].map((c) => c.command)
      // 创建新会话
      expect(commands.some((c) => ['/clear', '/new'].includes(c))).toBe(true)
      // 更换会话
      expect(commands.some((c) => ['/resume', '/sessions'].includes(c))).toBe(true)
      // 压缩上下文
      expect(commands).toContain('/compact')
      // 查看上下文（codex 用 /status、opencode 用 /details 替代）
      expect(commands.some((c) => ['/context', '/session', '/status', '/details'].includes(c))).toBe(true)
    }
  })

  it('skills slot: claude_code and pi use send mode, codex/opencode use substitutes', () => {
    const cc = AGENT_PRESETS.claude_code
    expect(cc.find((c) => c.command === '/')?.mode).toBe('send')
    const pi = AGENT_PRESETS.pi
    expect(pi.find((c) => c.command === '/skill:')?.mode).toBe('send')
    // 替代位为执行模式
    expect(AGENT_PRESETS.codex.find((c) => c.command === '/init')?.mode).toBe('execute')
    expect(AGENT_PRESETS.opencode.find((c) => c.command === '/templates')?.mode).toBe('execute')
  })
})

describe('getPresetCommandTexts', () => {
  it('returns 12 slash commands for each agent type', () => {
    for (const type of AGENT_TYPES) {
      const texts = getPresetCommandTexts(type)
      expect(texts).toHaveLength(12)
      expect(texts.every((c) => c.startsWith('/'))).toBe(true)
    }
  })

  it('returns empty list for generic (未识别无预设)', () => {
    expect(getPresetCommandTexts('generic')).toEqual([])
  })
})

describe('filterPresetCommands', () => {
  const commands = ['/model', '/new', '/compact', '/skill:', '/settings', '/']

  it('matches prefix case-insensitively', () => {
    expect(filterPresetCommands(commands, '/M')).toEqual(['/model'])
    expect(filterPresetCommands(commands, '/c')).toEqual(['/compact'])
    expect(filterPresetCommands(commands, '/skill')).toEqual(['/skill:'])
  })

  it('returns all candidates when only slash is typed', () => {
    const all = filterPresetCommands(commands, '/')
    expect(all).toContain('/model')
    expect(all).toContain('/settings')
    expect(all).toHaveLength(5)
  })

  it('excludes the bare slash command itself (skills 入口走面板)', () => {
    expect(filterPresetCommands(['/'], '/')).toEqual([])
  })

  it('returns empty for non-slash input or no match', () => {
    expect(filterPresetCommands(commands, 'model')).toEqual([])
    expect(filterPresetCommands(commands, '/zzz')).toEqual([])
    expect(filterPresetCommands(commands, '')).toEqual([])
  })
})
