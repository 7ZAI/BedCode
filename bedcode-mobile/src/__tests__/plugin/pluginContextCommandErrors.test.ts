/**
 * PluginContext 命令面拒绝值提取测试（ADR 0030 信封消费侧）
 *
 * 被测：`src/plugin/context.ts` 的 `commands.execute` WASM 桥回退路径
 * （内部 `extractInvokeErrorMessage` + 落盘包装）。
 *
 * 背景（真机缺陷）：宿主 AppError 经 Tauri IPC 以信封对象 `{code, request_id, params?}`
 * reject（真源 `bedcode-server-base::error::AppError::Serialize`，不含 message / 错误原文）；
 * 提取函数此前只认 message / 字符串，信封被当空 detail → 插件侧只见
 * `Command not found: <cmd>`，把真实失败（如 no active peer）掩盖成命令不存在。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-E1 | ADR 0030 信封 | `{code, request_id}` 拒绝值 | 消息带 `code, request_id=<id>`；params 不展开 |
 * | C-E2 | 非信封形态 | Error / 纯字符串 / `{message}` | 取对应文本（dev-shell / 单测 / 旧形态兼容） |
 * | C-E3 | 畸形兜底 | 空对象 / code 非字符串 / null / 数字 | 无 detail，仅 `Command not found: <id>` |
 * | C-E4 | 落盘 | 任意拒绝值 | frontendLogger.error 记命令 id + 原始拒绝值全量形状 |
 * | C-E5 | 本地 handler | 本地注册的 handler | 直接执行，不过桥、不落错误日志 |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'

const mockPluginInvoke = vi.fn()
vi.mock('@/plugin/commands', () => ({
  pluginInvoke: (...args: unknown[]) => mockPluginInvoke(...args),
}))

const mockLoggerError = vi.fn()
vi.mock('@/utils/frontendLogger', () => ({
  logger: {
    trace: vi.fn(),
    debug: vi.fn(),
    log: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: (...args: unknown[]) => mockLoggerError(...args),
  },
}))

import { createPluginContext } from '@/plugin/context'
import type { PluginInfo } from '@/plugin/types'

const PLUGIN_ID = 'com.bedcode.file-transfer'
const COMMAND = 'file-transfer.list-remote'

/** 最小插件信息（本测试只走 commands 面，其余字段不参与） */
function makeInfo(): PluginInfo {
  return {
    id: PLUGIN_ID,
    name: 'File Transfer',
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'wasm' as PluginInfo['pluginType'],
    permissions: [],
    state: 'activated' as PluginInfo['state'],
    contributes: {} as PluginInfo['contributes'],
    source: '',
    assetDir: '',
  } as PluginInfo
}

/** 取 rejection 的 Error 实例（断言完整消息，避免子串匹配漏掉多余内容） */
async function rejectionOf(promise: Promise<unknown>): Promise<Error> {
  return promise.then(
    () => {
      throw new Error('expected rejection, got resolution')
    },
    (e: unknown) => e as Error,
  )
}

describe('PluginContext 命令面拒绝值提取', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('C-E1 信封拒绝值：保留 code 与 request_id 关联键，params 不展开', async () => {
    const envelope = {
      code: 'host.plugin.trap',
      request_id: 'abc12345',
      params: { name: '文件传输' },
    }
    mockPluginInvoke.mockRejectedValueOnce(envelope)
    const ctx = createPluginContext(makeInfo())

    const err = await rejectionOf(ctx.commands.execute(COMMAND, { path: '', dirId: '' }))

    // 参数经单参直传（既有契约）
    expect(mockPluginInvoke).toHaveBeenCalledWith(PLUGIN_ID, COMMAND, { path: '', dirId: '' })
    // 完整消息：code + request_id；params 不展开（用户面文案归前端 i18n，此处只要排查标识）
    expect(err.message).toBe(`Command not found: ${COMMAND} (host.plugin.trap, request_id=abc12345)`)
  })

  it('C-E1 真机形态信封（host.internal + no active peer 场景）', async () => {
    mockPluginInvoke.mockRejectedValueOnce({ code: 'host.internal', request_id: 'ff09841a' })
    const ctx = createPluginContext(makeInfo())

    await expect(ctx.commands.execute(COMMAND, {})).rejects.toThrow(
      `Command not found: ${COMMAND} (host.internal, request_id=ff09841a)`,
    )
  })

  it('C-E1 request_id 缺失时只保留 code（不产出残缺的逗号）', async () => {
    mockPluginInvoke.mockRejectedValueOnce({ code: 'host.internal' })
    const ctx = createPluginContext(makeInfo())

    const err = await rejectionOf(ctx.commands.execute(COMMAND))

    expect(err.message).toBe(`Command not found: ${COMMAND} (host.internal)`)
  })

  it('C-E2 Error 实例拒绝值取 message（dev-shell / 单测环境）', async () => {
    mockPluginInvoke.mockRejectedValueOnce(new Error('wasm trap: unreachable executed'))
    const ctx = createPluginContext(makeInfo())

    await expect(ctx.commands.execute(COMMAND)).rejects.toThrow(
      `Command not found: ${COMMAND} (wasm trap: unreachable executed)`,
    )
  })

  it('C-E2 纯字符串拒绝值原样保留（整核前旧形态兼容）', async () => {
    mockPluginInvoke.mockRejectedValueOnce('no active peer')
    const ctx = createPluginContext(makeInfo())

    await expect(ctx.commands.execute(COMMAND)).rejects.toThrow(
      `Command not found: ${COMMAND} (no active peer)`,
    )
  })

  it('C-E2 带 message 的对象拒绝值取 message', async () => {
    mockPluginInvoke.mockRejectedValueOnce({ message: 'plugin not activated' })
    const ctx = createPluginContext(makeInfo())

    await expect(ctx.commands.execute(COMMAND)).rejects.toThrow(
      `Command not found: ${COMMAND} (plugin not activated)`,
    )
  })

  it('C-E3 畸形拒绝值一律无 detail（空对象 / code 非字符串 / null / 数字）', async () => {
    const ctx = createPluginContext(makeInfo())

    for (const malformed of [{}, { code: 42, request_id: 'x' }, null, 42]) {
      mockPluginInvoke.mockRejectedValueOnce(malformed)
      const err = await rejectionOf(ctx.commands.execute(COMMAND))
      expect(err.message, `畸形拒绝值 ${JSON.stringify(malformed)} 不应产出 detail`).toBe(
        `Command not found: ${COMMAND}`,
      )
    }
  })

  it('C-E4 任意拒绝值都落盘全量形状（信封对象原引用 + 命令 id）', async () => {
    const envelope = { code: 'host.internal', request_id: 'ff09841a' }
    mockPluginInvoke.mockRejectedValueOnce(envelope)
    const ctx = createPluginContext(makeInfo())

    await rejectionOf(ctx.commands.execute(COMMAND, {}))

    expect(mockLoggerError).toHaveBeenCalledTimes(1)
    const [message, logged] = mockLoggerError.mock.calls[0]!
    expect(String(message)).toContain(COMMAND)
    // 原始拒绝值原样落盘（不是提取后的字符串）：信封字段在日志里保持可 grep
    expect(logged).toBe(envelope)
  })

  it('C-E5 本地 handler 直接执行：不过 WASM 桥、不落错误日志', async () => {
    const ctx = createPluginContext(makeInfo())
    ctx.commands.register('local.cmd', (a: number, b: number) => a + b)

    await expect(ctx.commands.execute('local.cmd', 1, 2)).resolves.toBe(3)

    expect(mockPluginInvoke).not.toHaveBeenCalled()
    expect(mockLoggerError).not.toHaveBeenCalled()
  })
})
