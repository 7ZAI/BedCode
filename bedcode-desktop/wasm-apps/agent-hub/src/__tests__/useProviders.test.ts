/**
 * useProviders 供应商域行为契约（2026-10-04 OCR A-03 回归）
 *
 * 覆盖：fetchModels 的条目校验（空列表 / 非字符串条目 / 空白条目 → 显式失败，
 * 不把「0 个模型」或坏形状当作成功写入目标配置；合法条目 trim 后回传）。
 * 断言全部是外部可见行为——发往 guest 的命令名与入参、按返回值作出的
 * status 判别；不测内部实现。
 *
 * 契约清单：
 * - F1 fetchModels 发送 agent-hub.fetch-models（URL / presetId / apiKey 透传）
 * - F2 空回执（null / 无 models / 空数组）→ status: error（不假装查到模型）
 * - F3 条目含非字符串（A-03：mergeModelIds 的 id.trim() 会 TypeError）
 *   → 整个回执拒绝，绝不悄悄丢条目
 * - F4 条目全是空白字符串 → status: error
 * - F5 合法条目（含前后空白）→ status: ok 且 models 已 trim
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import {
  useProviders,
  type UseProvidersReturn,
} from '../composables/useProviders'

const execute = vi.fn()

function makeContext(): PluginContext {
  return {
    i18n: { t: (k: string) => k, getI18n: () => undefined },
    commands: { execute },
    events: { on: () => ({ dispose: () => {} }) },
  } as unknown as PluginContext
}

function mountComposable() {
  let api: UseProvidersReturn | null = null
  mount(
    defineComponent({
      setup() {
        api = useProviders(makeContext())
        return () => h('div')
      },
    }),
  )
  return api!
}

describe('useProviders.fetchModels（模型列表查询条目校验）', () => {
  beforeEach(() => {
    execute.mockReset()
  })

  it('F1：发送 fetch-models 命令，载荷原样透传', async () => {
    execute.mockResolvedValue({ models: ['m1'], url: 'https://u/v1/models', count: 1 })
    const api = mountComposable()

    const res = await api.fetchModels({ url: 'https://u/v1/models', presetId: 3, apiKey: 'k' })
    expect(execute).toHaveBeenCalledWith('agent-hub.fetch-models', {
      url: 'https://u/v1/models',
      presetId: 3,
      apiKey: 'k',
    })
    expect(res.status).toBe('ok')
    api!.fetchingModels.value // noop: 防未使用
  })

  it('F2 反例：空回执（null / 缺 models / 空数组）→ error 而非 ok', async () => {
    for (const empty of [null, {}, { models: [], url: 'x', count: 0 }]) {
      execute.mockReset()
      execute.mockResolvedValue(empty)
      const api = mountComposable()
      const res = await api.fetchModels({ url: 'https://u/v1/models' })
      expect(res.status).toBe('error')
    }
  })

  it('F3 反例（A-03）：条目含非字符串 → error，不把坏形状当作 0 模型', async () => {
    execute.mockResolvedValue({
      models: ['m1', 42 as unknown as string, { id: 'x' } as unknown as string],
      url: 'https://u/v1/models',
      count: 3,
    })
    const api = mountComposable()

    const res = await api.fetchModels({ url: 'https://u/v1/models' })
    expect(res.status).toBe('error')
    // 校验必须发生在「data 写回调用方」之前：绝不能带着坏条目返回 ok
    expect(res).not.toMatchObject({ status: 'ok' })
  })

  it('F4 反例：条目全是空白字符串 → error（空白条目与「0 个模型」同害）', async () => {
    execute.mockResolvedValue({
      models: ['   ', '', '\t'],
      url: 'https://u/v1/models',
      count: 3,
    })
    const api = mountComposable()

    const res = await api.fetchModels({ url: 'https://u/v1/models' })
    expect(res.status).toBe('error')
  })

  it('F5 正例：合法条目 trim 后回传（chip 与手动列表比对不漂移）', async () => {
    execute.mockResolvedValue({
      models: ['  m1  ', 'm2', ' m3\n'],
      url: 'https://u/v1/models',
      count: 3,
    })
    const api = mountComposable()

    const res = await api.fetchModels({ url: 'https://u/v1/models' })
    expect(res.status).toBe('ok')
    if (res.status !== 'ok') return
    expect(res.data?.models).toEqual(['m1', 'm2', 'm3'])
  })
})