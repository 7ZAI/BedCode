/**
 * useNotification 单测:设置页通知开关的实际生效链路
 *
 * 覆盖 5 个设置开关到 showTaskNotification / showConnectionNotification
 * invoke 参数的映射,以及 notifyOnConnection / notifyInBackground 的
 * 门控行为(此前这两个开关无消费点,属死设置):
 * - notifyOnConnection=false:连接类通知不发
 * - notifyInBackground=false:后台也不发(前台由界面反馈,始终不发)
 * - 前台(visibilityState=visible)一律不发系统通知
 * - vibrate / soundOnTaskComplete / notifyOnWaiting 决定渠道选择 flags
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const mockInvoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

vi.mock('@/composables/usePlatform', () => ({
  usePlatform: () => ({
    platformInfo: { value: { platform: 'android' } },
  }),
}))

vi.mock('@/locales', () => ({
  default: { global: { t: (key: string) => key } },
}))

import { useNotification } from '@/composables/useNotification'

// ==================== 测试基建 ====================

/** 控制可见性:jsdom 默认 visible(前台);hidden 模拟切后台/灭屏/锁屏 */
function setVisibility(state: 'visible' | 'hidden'): void {
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: state })
}

/** 写入 localStorage 设置种子,缺省字段由 useNotification 侧 ?? true 兜底 */
function seedSettings(overrides: Record<string, unknown> = {}): void {
  localStorage.setItem('mobile-settings', JSON.stringify({ ...overrides }))
}

/** 取 invoke 调用中指定命令的参数 */
function invocationsOf(cmd: string): Array<Record<string, unknown>> {
  return mockInvoke.mock.calls
    .filter(([c]) => c === cmd)
    .map(([, args]) => args as Record<string, unknown>)
}

beforeEach(() => {
  vi.clearAllMocks()
  setVisibility('hidden')
  localStorage.clear()
  // 权限链路:已授予,后续 show* 命令 resolve
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'plugin:task-notification|checkNotificationPermission') return { granted: true }
    if (cmd === 'plugin:task-notification|requestNotificationPermission') return { granted: true }
    return undefined
  })
})

// ==================== notifyOnConnection ====================

describe('showConnectionNotification notifyOnConnection 门控', () => {
  it('后台 + 默认开关:发连接通知', async () => {
    setVisibility('hidden')
    seedSettings()
    const { showConnectionNotification } = useNotification()
    await showConnectionNotification({ type: 'disconnected', deviceName: 'PC' })
    expect(invocationsOf('plugin:task-notification|showConnectionNotification')).toHaveLength(1)
  })

  it('后台 + notifyOnConnection=false:不发连接通知', async () => {
    setVisibility('hidden')
    seedSettings({ notifyOnConnection: false })
    const { showConnectionNotification } = useNotification()
    await showConnectionNotification({ type: 'disconnected', deviceName: 'PC' })
    expect(invocationsOf('plugin:task-notification|showConnectionNotification')).toHaveLength(0)
  })

  it('前台 + 默认开关:不发连接通知(界面已有 toast 反馈)', async () => {
    setVisibility('visible')
    seedSettings()
    const { showConnectionNotification } = useNotification()
    await showConnectionNotification({ type: 'reconnect_failed' })
    expect(invocationsOf('plugin:task-notification|showConnectionNotification')).toHaveLength(0)
  })

  it('后台 + notifyInBackground=false:不发连接通知', async () => {
    setVisibility('hidden')
    seedSettings({ notifyInBackground: false })
    const { showConnectionNotification } = useNotification()
    await showConnectionNotification({ type: 'auth_failed' })
    expect(invocationsOf('plugin:task-notification|showConnectionNotification')).toHaveLength(0)
  })

  it('发出时震动受 vibrate 开关控制,声音始终开启', async () => {
    setVisibility('hidden')
    seedSettings({ vibrate: false })
    const { showConnectionNotification } = useNotification()
    await showConnectionNotification({ type: 'disconnected', deviceName: 'PC' })
    const args = invocationsOf('plugin:task-notification|showConnectionNotification')[0]
    expect(args.vibrate).toBe(false)
    expect(args.sound).toBe(true)
  })
})

// ==================== notifyInBackground(任务通知) ====================

describe('showTaskNotification notifyInBackground 门控', () => {
  it('后台 + 默认开关:任务完成通知发出且声震全开', async () => {
    setVisibility('hidden')
    seedSettings()
    const { showTaskNotification } = useNotification()
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: 'completed' })
    const args = invocationsOf('plugin:task-notification|showTaskNotification')[0]
    expect(args.vibrate).toBe(true)
    expect(args.sound).toBe(true)
  })

  it('前台 + 默认开关:任务通知不发', async () => {
    setVisibility('visible')
    seedSettings()
    const { showTaskNotification } = useNotification()
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: 'completed' })
    expect(invocationsOf('plugin:task-notification|showTaskNotification')).toHaveLength(0)
  })

  it('后台 + notifyInBackground=false:任务通知不发', async () => {
    setVisibility('hidden')
    seedSettings({ notifyInBackground: false })
    const { showTaskNotification } = useNotification()
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: 'interrupted' })
    expect(invocationsOf('plugin:task-notification|showTaskNotification')).toHaveLength(0)
  })
})

// ==================== vibrate / soundOnTaskComplete / notifyOnWaiting ====================

describe('showTaskNotification 提醒 flags 映射', () => {
  it.each([
    { desc: 'notifyOnWaiting=false:asking 走静默渠道', status: 'asking', overrides: { notifyOnWaiting: false }, flags: { vibrate: false, sound: false } },
    { desc: 'soundOnTaskComplete=false:completed 走静默渠道', status: 'completed', overrides: { soundOnTaskComplete: false }, flags: { vibrate: false, sound: false } },
    { desc: 'vibrate=false + soundOnTaskComplete=false:interrupted 也静默', status: 'interrupted', overrides: { vibrate: false, soundOnTaskComplete: false }, flags: { vibrate: false, sound: false } },
    { desc: 'vibrate=false 仅声:completed 走仅声音组合', status: 'completed', overrides: { vibrate: false }, flags: { vibrate: false, sound: true } },
  ])('$desc', async ({ status, overrides, flags }) => {
    setVisibility('hidden')
    seedSettings(overrides)
    const { showTaskNotification } = useNotification()
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: status })
    const args = invocationsOf('plugin:task-notification|showTaskNotification')[0]
    expect(args.vibrate).toBe(flags.vibrate)
    expect(args.sound).toBe(flags.sound)
  })

  it('idle / in_progress 状态一律不发', async () => {
    setVisibility('hidden')
    seedSettings()
    const { showTaskNotification } = useNotification()
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: 'idle' })
    await showTaskNotification({ sessionId: 's1', sessionName: 'dev', taskStatus: 'in_progress' })
    expect(invocationsOf('plugin:task-notification|showTaskNotification')).toHaveLength(0)
  })
})
