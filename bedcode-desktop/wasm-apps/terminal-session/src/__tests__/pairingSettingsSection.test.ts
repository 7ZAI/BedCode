/**
 * PairingSettingsSection 行为契约（票 14，插件设置分组正文）
 *
 * 契约来源：宿主 `SettingsPairingSection.vue`（退役前的行为基线）+ spec D6
 * 「两层配置口径」与「校验不降级」。
 *
 * 契约清单：
 * - C1 挂载：经 `session.settings.ttl.get` 读取两项有效期并回显
 * - C2 失焦保存：经 `session.settings.ttl.set` 提交 `{key, value}`（键名与宿主原页同形）
 * - C3 越界收敛：低于 60 / 高于 3600 的值收敛到边界后提交（与宿主同口径）
 * - C4 保存失败：以错误文案提示；成功路径必须有成功提示（两者互斥）
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import PairingSettingsSection from '../components/PairingSettingsSection.vue'
import devMock from '../devMock'

vi.mock('vue-sonner', () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}))

const seed = devMock.pairing

async function defaultExecute(command: string, _args?: unknown) {
  switch (command) {
    case 'session.settings.ttl.get':
      return seed.ttls
    case 'session.settings.ttl.set':
      return seed.ttls
    default:
      throw new Error(`unexpected command: ${command}`)
  }
}

const execute = vi.fn(defaultExecute)

function makeContext(): PluginContext {
  return {
    id: 'com.bedcode.terminal-session',
    i18n: {
      t: (key: string) => key,
      getI18n: () => ({ global: { locale: { value: 'zh-CN' }, t: (key: string) => key } }),
      registerMessages: vi.fn(),
    },
    commands: { execute },
  } as unknown as PluginContext
}

function mountView() {
  return mount(PairingSettingsSection, {
    global: { provide: { pluginContext: makeContext() } },
  })
}

function commandsTo(id: string) {
  return execute.mock.calls.filter((c) => c[0] === id)
}

/** 两个输入框：[0] 二维码有效期，[1] 配对码有效期（与宿主原页顺序一致） */
function inputs(wrapper: ReturnType<typeof mountView>) {
  return wrapper.findAll('input')
}

async function setValueAndBlur(input: ReturnType<typeof inputs>[number], value: string) {
  await input.setValue(value)
  await input.trigger('blur')
  await flushPromises()
}

describe('PairingSettingsSection（设置页配对分组正文）', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    execute.mockImplementation(defaultExecute)
  })

  it('C1 挂载：读取两项有效期并回显到输入框', async () => {
    const wrapper = mountView()
    await flushPromises()

    expect(commandsTo('session.settings.ttl.get')).toHaveLength(1)
    const [qr, code] = inputs(wrapper)
    expect((qr.element as HTMLInputElement).value).toBe(String(seed.ttls.qrTokenTtl))
    expect((code.element as HTMLInputElement).value).toBe(String(seed.ttls.pairingCodeTtl))
    // 两项各自的标签文案（i18n key 直返）
    expect(wrapper.text()).toContain('pairing.settings.qrValidity')
    expect(wrapper.text()).toContain('pairing.settings.pairingCodeTtl')
  })

  it('C2 失焦保存：分别提交 qrTokenTtl 与 pairingCodeTtl 两项', async () => {
    const wrapper = mountView()
    await flushPromises()
    const [qr, code] = inputs(wrapper)
    const { toast } = await import('vue-sonner')

    await setValueAndBlur(qr, '600')
    expect(commandsTo('session.settings.ttl.set')[0][1]).toEqual({
      key: 'qrTokenTtl',
      value: 600,
    })

    await setValueAndBlur(code, '120')
    expect(commandsTo('session.settings.ttl.set')[1][1]).toEqual({
      key: 'pairingCodeTtl',
      value: 120,
    })
    expect(toast.success).toHaveBeenCalledWith('pairing.settings.saved')
  })

  it('C3 越界收敛：低于下限收敛到 60、高于上限收敛到 3600（提交值与回显值同源）', async () => {
    const wrapper = mountView()
    await flushPromises()
    const [qr, code] = inputs(wrapper)

    await setValueAndBlur(qr, '10')
    expect(commandsTo('session.settings.ttl.set')[0][1]).toEqual({
      key: 'qrTokenTtl',
      value: 60,
    })
    expect((qr.element as HTMLInputElement).value).toBe('60')

    await setValueAndBlur(code, '99999')
    expect(commandsTo('session.settings.ttl.set')[1][1]).toEqual({
      key: 'pairingCodeTtl',
      value: 3600,
    })
    expect((code.element as HTMLInputElement).value).toBe('3600')
  })

  it('C4 保存失败：以错误文案提示且不报告成功', async () => {
    const wrapper = mountView()
    await flushPromises()
    execute.mockImplementation(async (command: string) => {
      if (command === 'session.settings.ttl.set') throw new Error('denied')
      return defaultExecute(command)
    })
    const { toast } = await import('vue-sonner')

    await setValueAndBlur(inputs(wrapper)[0], '600')

    expect(toast.error).toHaveBeenCalledWith('pairing.settings.saveFailed')
    expect(toast.success).not.toHaveBeenCalled()
  })
})
