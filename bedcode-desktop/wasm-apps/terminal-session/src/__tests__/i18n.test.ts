/**
 * 插件 i18n 契约（票 13）
 *
 * 契约来源：spec D6「插件侧扁平 key 必须先加命名空间前缀（`session.*` / `task.*` /
 * `pairing.*` / `settings.session.*`）」+ AGENTS §6「新增/修改 key 必须同步出现在
 * zh-CN 与 en 两文件」。
 *
 * 编译期护栏：两份语言文件都实现 `MessageSchema`（漏 key 即 TS 报错）；本测试守住
 * 运行期无法静态证明的两件事：语言表键集逐字相等（含同名 key 不同大小写拼写）、
 * 键全部落在本票的 `session.` 命名空间内（防与宿主文案在同一注册表下互相覆盖）。
 */

import { describe, it, expect } from 'vitest'
import { messages } from '../i18n'

const zhKeys = Object.keys(messages['zh-CN']).sort()
const enKeys = Object.keys(messages['en']).sort()

describe('插件 i18n 键集', () => {
  it('zh-CN 与 en 键集逐字相等（含数量）', () => {
    expect(enKeys).toEqual(zhKeys)
    expect(zhKeys.length).toBeGreaterThan(40)
  })

  it('所有键落在 session.* / pairing.* / task.* 命名空间（注册时再经插件 ID 前缀隔离）', () => {
    // spec D6 按域前缀：会话域 session.*（票 13）、设备与配对域 pairing.*（票 14）、
    // 任务域 task.*（票 17：旧 auto-task 插件的扁平 key 加前缀后并入）
    const stray = [...zhKeys, ...enKeys].filter(
      (k) => !k.startsWith('session.') && !k.startsWith('pairing.') && !k.startsWith('task.'),
    )
    expect(stray, `越界键：${stray.join(', ')}`).toEqual([])
  })

  it('每个键都有非空文案（两语言均不为空串）', () => {
    for (const key of zhKeys) {
      expect(messages['zh-CN'][key as keyof (typeof messages)['zh-CN']], key).toBeTruthy()
      expect(messages['en'][key as keyof (typeof messages)['en']], key).toBeTruthy()
    }
  })

  it('会话页文案与宿主原页逐字同源（抽查搬走后不得改写的用户可见文案）', () => {
    // 抽 3 条：会话页标题、空态、终端入口 —— 搬迁只改归属，不改文案
    expect(messages['zh-CN']['session.sidebar.title']).toBe('终端会话')
    expect(messages['zh-CN']['session.empty.noConfig']).toBe('暂无会话配置')
    expect(messages['zh-CN']['session.terminal.view']).toBe('查看终端')
  })
})
