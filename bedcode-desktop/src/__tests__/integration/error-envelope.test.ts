/**
 * 错误信封跨边界集成测试（票 01，票 05 启用，随全量回归执行）
 *
 * 垂直切片：Rust AppError 信封形状 → invoke rejection → useServer → ServerView →
 * showUserError → toast 友好文案（无技术原文 / 无 request_id / 无命令名）+ logger 落盘。
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import ServerView from '@/views/ServerView.vue'
import { useServer } from '@/composables/useServer'
import { logger } from '@/utils/frontendLogger'
import { makeServerStatusInfo, makeNetworkConfig } from '@/__tests__/fixtures/index'

const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

vi.mock('vue-echarts', () => ({
  default: { name: 'VChartStub', template: '<div class="vchart-stub" />' },
}))

vi.mock('vue-sonner', () => ({
  toast: {
    success: vi.fn(() => 'mock-id'),
    error: vi.fn(() => 'mock-id-error'),
    warning: vi.fn(() => 'mock-id'),
    info: vi.fn(() => 'mock-id'),
  },
}))

import { toast } from 'vue-sonner'
const mockedToast = vi.mocked(toast)

async function flushAsync(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0))
}

let wrapper: ReturnType<typeof mount> | null = null
let pinia: ReturnType<typeof createPinia>
let errorSpy: ReturnType<typeof vi.spyOn>

function installInvokeMock() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case 'get_server_status':
        return Promise.resolve(makeServerStatusInfo({ status: 'stopped', port: 8765 }))
      case 'get_server_network_config':
        return Promise.resolve(makeNetworkConfig({ port: 8765 }))
      default:
        return Promise.resolve(undefined)
    }
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  pinia = createPinia()
  setActivePinia(pinia)
  installInvokeMock()
  const server = useServer()
  server.status.value = 'stopped'
  server.stopPolling()
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  errorSpy.mockRestore()
})

function buttonByText(text: string) {
  return wrapper!.findAll('button').find((b) => b.text().trim() === text)!
}

describe('错误信封垂直切片', () => {
  it('启动失败：toast 只显示友好文案，无技术原文/request_id/命令名；日志带追踪号', async () => {
    wrapper = mount(ServerView, { global: { plugins: [pinia, i18n] } })
    await flushAsync()

    // 后端失败返回信封（模拟 Rust AppError::Internal 序列化，host.internal 兜底）
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'server_start') {
        return Promise.reject({ code: 'host.internal', request_id: 'deadbeef' })
      }
      return Promise.resolve(undefined)
    })

    await buttonByText('启动').trigger('click')
    await flushAsync()

    // toast 文案 = errors.host.internal（zh），不含任何技术痕迹
    const message = mockedToast.error.mock.calls[0][0] as string
    expect(message).toBe('操作未完成，请稍后重试')
    expect(message).not.toContain('deadbeef')
    expect(message).not.toContain('host.internal')
    expect(message).not.toContain('server_start')

    // 日志：code + request_id 全量落地（技术详情唯一出口；arg1=消息串带 request_id，
    // arg2=归一化后的 UserError（requestId 字段），与宿主日志按值关联）
    expect(errorSpy).toHaveBeenCalledWith(
      expect.stringContaining('code=host.internal'),
      expect.objectContaining({ requestId: 'deadbeef' }),
    )
  })

  it('启动超时：toast 显示超时文案并带「重试」按钮，重试后成功', async () => {
    wrapper = mount(ServerView, { global: { plugins: [pinia, i18n] } })
    await flushAsync()

    let calls = 0
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'server_start') {
        calls += 1
        return calls === 1
          ? Promise.reject({ code: 'host.invoke.timeout', request_id: 'beefcafe', params: { seconds: 30 } })
          : Promise.resolve(undefined)
      }
      return Promise.resolve(undefined)
    })

    await buttonByText('启动').trigger('click')
    await flushAsync()

    const message = mockedToast.error.mock.calls[0][0] as string
    expect(message).toBe('操作超时，请重试')
    const options = mockedToast.error.mock.calls[0][1]
    expect(options.action).toEqual({
      label: '重试',
      onClick: expect.any(Function),
    })

    // 点击重试 → 重发 server_start → 成功路径
    options.action.onClick()
    await flushAsync()
    expect(useServer().status.value).toBe('running')
  })
})