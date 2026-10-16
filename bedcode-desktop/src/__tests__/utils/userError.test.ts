/**
 * 前端错误消费层测试（ADR 0030）：parseInvokeError 形状矩阵 / showUserError 行为 /
 * userErrorFromUnknown 兜底 —— 硬不变量：toast 永不渲染 code / request_id / 技术详情。
 *
 * seam：
 * - vue-sonner toast 模块级 mock（断言 toast.error 的消息与选项）
 * - logger.error spy（断言日志落点 = 技术详情唯一前端出口）
 * - i18n 用真实实例（src/locales，zh-CN 默认）——顺带锁注册表 v0 文案 + zh/en 同步
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import i18n from '@/locales'
import { logger } from '@/utils/frontendLogger'
import {
  UserError,
  parseInvokeError,
  showUserError,
  userErrorFromUnknown,
  HOST_INTERNAL_CODE,
  FRONTEND_INTERNAL_CODE,
  IPC_TIMEOUT_CODE,
} from '@/utils/userError'

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

let errorSpy: ReturnType<typeof vi.spyOn>

beforeEach(() => {
  vi.clearAllMocks()
  errorSpy = vi.spyOn(logger, 'error').mockImplementation(() => {})
})

afterEach(() => {
  errorSpy.mockRestore()
})

describe('parseInvokeError：形状矩阵', () => {
  it('正例：对象信封完整映射三字段', () => {
    const ue = parseInvokeError({ code: 'host.plugin.trap', request_id: 'a1b2c3d4', params: { name: 'x' } })
    expect(ue).toBeInstanceOf(UserError)
    expect(ue.code).toBe('host.plugin.trap')
    expect(ue.requestId).toBe('a1b2c3d4')
    expect(ue.params).toEqual({ name: 'x' })
  })

  it('正例：对象信封省略 request_id/params 时得 undefined', () => {
    const ue = parseInvokeError({ code: 'host.internal' })
    expect(ue.code).toBe('host.internal')
    expect(ue.requestId).toBeUndefined()
    expect(ue.params).toBeUndefined()
  })

  it('反例：字符串（遗留 rejection）→ host.internal 兜底', () => {
    expect(parseInvokeError('legacy plain error').code).toBe(HOST_INTERNAL_CODE)
  })

  it('反例：Error 实例 → host.internal 兜底', () => {
    expect(parseInvokeError(new Error('backend crashed')).code).toBe(HOST_INTERNAL_CODE)
  })

  it('边界：null / undefined → host.internal 兜底', () => {
    expect(parseInvokeError(null).code).toBe(HOST_INTERNAL_CODE)
    expect(parseInvokeError(undefined).code).toBe(HOST_INTERNAL_CODE)
  })

  it('反例：无 code 的对象 → host.internal 兜底', () => {
    expect(parseInvokeError({ error: 'nope' }).code).toBe(HOST_INTERNAL_CODE)
  })

  it('反例：code 非字符串（数字）→ host.internal 兜底', () => {
    expect(parseInvokeError({ code: 404 }).code).toBe(HOST_INTERNAL_CODE)
    expect(parseInvokeError({ code: { nested: 'x' } }).code).toBe(HOST_INTERNAL_CODE)
  })

  it('幂等：UserError 实例原样返回', () => {
    const original = new UserError('host.invoke.timeout', { seconds: 30 }, 'abc12345')
    expect(parseInvokeError(original)).toBe(original)
  })
})

describe('showUserError：友好提示 + 日志 + 重试', () => {
  it('正例：已知 code → toast 显示 errors.* 文案，日志带 code + request_id', () => {
    const ue = showUserError({
      code: 'host.internal',
      request_id: 'deadbeef',
    })
    expect(mockedToast.error).toHaveBeenCalledWith('操作未完成，请稍后重试', { duration: 5000 })
    expect(errorSpy).toHaveBeenCalledWith(
      '[user-error] code=host.internal request_id=deadbeef',
      expect.objectContaining({ code: 'host.internal', request_id: 'deadbeef' }),
    )
    expect(ue.code).toBe('host.internal')
  })

  it('反例（关键）：toast 文案永不包含 code / request_id / 技术详情', () => {
    const detail = new Error('connection refused at 10.0.0.1:8765')
    showUserError({ code: 'host.internal', request_id: 'cafe1234' }, detail as never)
    const message = mockedToast.error.mock.calls[0][0] as string
    expect(message).not.toContain('host.internal')
    expect(message).not.toContain('cafe1234')
    expect(message).not.toContain('connection refused')
    expect(message).not.toContain('10.0.0.1')
  })

  it('边界：未知 code（expand 阶段业务码无文案）回退 fallbackCode', () => {
    showUserError({ code: 'host.plugin.whatever', request_id: 'x' }, { fallbackCode: 'frontend.internal' } as never)
    expect(mockedToast.error).toHaveBeenCalledWith('操作未完成，请稍后重试', expect.anything())
  })

  it('边界：未知 code 且无 fallbackCode → host.internal 兜底文案', () => {
    showUserError({ code: 'some.future.code' })
    expect(mockedToast.error).toHaveBeenCalledWith('操作未完成，请稍后重试', expect.anything())
  })

  it('正例：IPC 超时 + retry 回调 → toast 提供「重试」按钮', () => {
    const retry = vi.fn()
    showUserError(new UserError(IPC_TIMEOUT_CODE, { seconds: 30 }), { retry })
    expect(mockedToast.error).toHaveBeenCalledWith(
      '操作超时，请重试',
      expect.objectContaining({
        action: { label: '重试', onClick: retry },
      }),
    )
  })

  it('反例：IPC 超时无 retry 回调 → 无重试按钮', () => {
    showUserError(new UserError(IPC_TIMEOUT_CODE))
    const options = mockedToast.error.mock.calls[0][1]
    expect(options.action).toBeUndefined()
  })

  it('反例：非超时码即使提供 retry 回调也不显示按钮（v1 仅该码可重试）', () => {
    showUserError(new UserError('host.internal'), { retry: () => {} })
    const options = mockedToast.error.mock.calls[0][1]
    expect(options.action).toBeUndefined()
  })

  it('正例：返回归一化 UserError 供调用方继续使用', () => {
    const result = showUserError({ code: 'host.internal', request_id: 'abc' })
    expect(result).toBeInstanceOf(UserError)
    expect(result.code).toBe('host.internal')
    expect(result.requestId).toBe('abc')
  })

  it('en locale 同步：errors.* 基码文案存在', () => {
    i18n.global.locale.value = 'en'
    try {
      expect(i18n.global.t('errors.host.internal')).toBe('Operation failed, please try again')
      expect(i18n.global.t('errors.host.invoke.timeout')).toBe('Operation timed out, please retry')
      expect(i18n.global.t('errors.retry')).toBe('Retry')
      // 票 02 机制码（zh + en 同步）
      expect(i18n.global.t('errors.host.plugin.not-activated')).toBe(
        'This app is not enabled, this action is unavailable',
      )
      expect(i18n.global.t('errors.host.plugin.not-found')).toBe('App not found or removed')
    } finally {
      i18n.global.locale.value = 'zh-CN'
    }
  })

  describe('showUserError：插件注册码（ADR 0030 决定 4 裸 key 回退）', () => {
    // 插件域码 = 插件 registerMessages 后的完整 key；宿主 i18n 不预埋，
    // 消费层以裸 key 回退查找（零映射层）。此处模拟插件已注册该 key。
    const PLUGIN_CODE = 'com.bedcode.terminal-session.session.error.sessionNotFound'

    beforeAll(() => {
      i18n.global.mergeLocaleMessage('zh-CN', { [PLUGIN_CODE]: '会话不存在或已结束' })
      i18n.global.mergeLocaleMessage('en', { [PLUGIN_CODE]: 'Session not found or already ended' })
    })

    it('正例：插件码 → toast 显示插件自己的 i18n 文案（宿主不预埋）', () => {
      showUserError({ code: PLUGIN_CODE, request_id: 'abc12345', params: { sessionId: 's-1' } })
      expect(mockedToast.error).toHaveBeenCalledWith('会话不存在或已结束', expect.anything())
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining('code=' + PLUGIN_CODE),
        expect.objectContaining({ code: PLUGIN_CODE }),
      )
    })

    it('正例：插件码 en locale 同步', () => {
      i18n.global.locale.value = 'en'
      try {
        showUserError({ code: PLUGIN_CODE })
        expect(mockedToast.error).toHaveBeenCalledWith('Session not found or already ended', expect.anything())
      } finally {
        i18n.global.locale.value = 'zh-CN'
      }
    })

    it('反例：插件码 + 宿主均无文案 → 兜底文案（expand 阶段）', () => {
      showUserError({ code: 'com.bedcode.other.missing-code' })
      expect(mockedToast.error).toHaveBeenCalledWith('操作未完成，请稍后重试', expect.anything())
    })
  })
})

describe('userErrorFromUnknown：renderer 侧兜底', () => {
  it('返回 frontend.internal 并保留原文到日志', () => {
    const err = new Error('pure frontend bug with details')
    const ue = userErrorFromUnknown(err)
    expect(ue.code).toBe(FRONTEND_INTERNAL_CODE)
    expect(errorSpy).toHaveBeenCalledWith('[user-error] unhandled renderer error:', err)
  })

  it('非 Error 值也能兜底（不抛错）', () => {
    expect(userErrorFromUnknown('oops').code).toBe(FRONTEND_INTERNAL_CODE)
    expect(userErrorFromUnknown(undefined).code).toBe(FRONTEND_INTERNAL_CODE)
  })
})