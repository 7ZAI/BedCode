/**
 * terminalIdle 空闲提示符判定单元测试
 *
 * 覆盖：常见 CLI 提示符（CC/pi/opencode/bash/zsh/cmd/PowerShell）、
 * 确认类提问、普通流式输出不误判。
 */
import { describe, it, expect } from 'vitest'
import { isIdlePromptLine } from '@/utils/terminalIdle'

describe('isIdlePromptLine', () => {
  it('detects common CLI prompt markers at line start', () => {
    expect(isIdlePromptLine('> ')).toBe(true)
    expect(isIdlePromptLine('>')).toBe(true)
    expect(isIdlePromptLine('❯ ')).toBe(true)
    expect(isIdlePromptLine('codex> ')).toBe(true)
    expect(isIdlePromptLine('pi> fix the bug')).toBe(true)
  })

  it('detects shell prompts (bash/zsh/cmd/PowerShell)', () => {
    expect(isIdlePromptLine('user@host:~/proj$ ')).toBe(true)
    expect(isIdlePromptLine('root@host:/opt# ')).toBe(true)
    expect(isIdlePromptLine('user@host ~% ')).toBe(true)
    expect(isIdlePromptLine('C:\\Users\\binblink>')).toBe(true)
    expect(isIdlePromptLine('PS C:\\Users\\binblink> ')).toBe(true)
  })

  it('detects confirmation questions', () => {
    expect(isIdlePromptLine('(Y/n)')).toBe(true)
    expect(isIdlePromptLine('Allow Claude to edit files? (Y/n)')).toBe(true)
    expect(isIdlePromptLine('[y/N]')).toBe(true)
    expect(isIdlePromptLine('Press any key to continue')).toBe(true)
    expect(isIdlePromptLine('Should I proceed?')).toBe(true)
  })

  it('rejects plain streaming output lines', () => {
    expect(isIdlePromptLine('Analyzing the codebase for bugs...')).toBe(false)
    expect(isIdlePromptLine('3 files changed, 42 insertions')).toBe(false)
    expect(isIdlePromptLine('$ pip install x')).toBe(false)
    expect(isIdlePromptLine('')).toBe(false)
  })
})
