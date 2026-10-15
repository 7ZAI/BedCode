/**
 * 业务设置页 渲染与交互契约测试
 * （2026-10-15：修复「首选认证方式不回显」——分段按钮把**设置键**当成了选项值比较）
 *
 * 被测：`src/settings/SettingsPage.vue`（挂载真实组件 + 替身宿主 KV 桥）。
 * 替身边界：只 mock 宿主 KV 桥（getMobileApi 的 readAllSettings / writeSetting）。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-SP1 默认回显 | KV 无 `mobile.preferredAuthMethod` | 按定义默认值 `pairing_code` 选中 | 「配对码」段 aria-pressed=true、生物段 false |
 * | C-SP2 落库回显 | KV 有 `mobile.preferredAuthMethod=biometric` | 选中项跟随 KV | 生物段 true、配对码段 false |
 * | C-SP3 点击落库 | 点「生物识别」段 | 归一后写 KV 并更新本地值 | writeSetting('mobile.preferredAuthMethod','biometric') + 选中态翻转 |
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

const readAllSettings = vi.fn()
const writeSetting = vi.fn()

vi.mock('@binblink/bedcode-plugin-sdk-mobile', async () => {
  const actual = await vi.importActual<typeof import('@binblink/bedcode-plugin-sdk-mobile')>(
    '@binblink/bedcode-plugin-sdk-mobile',
  )
  return { ...actual, getMobileApi: () => ({ readAllSettings, writeSetting }) }
})

const { default: SettingsPage } = await import('../SettingsPage.vue')

/** 最小插件上下文替身（i18n 回显 key；toast/confirm 记录调用） */
function makePluginContext() {
  return {
    id: 'com.bedcode.terminal-session',
    i18n: { t: (key: string) => key },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
    dialogs: { showToast: vi.fn(), showConfirm: vi.fn(async () => true) },
  } as unknown as PluginContext
}

/** 认证方式分段按钮（按文案 key 定位；settingsModel 里的选项键固定） */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function authSegments(wrapper: any) {
  return wrapper.findAll('button[aria-pressed]') as any[]
}

async function mountPage() {
  const wrapper = mount(SettingsPage, {
    global: { provide: { pluginContext: makePluginContext() } },
  })
  // load() 经 onMounted 异步执行
  await Promise.resolve()
  await nextTick()
  return wrapper
}

beforeEach(() => {
  readAllSettings.mockReset()
  writeSetting.mockReset()
  readAllSettings.mockResolvedValue({})
  writeSetting.mockResolvedValue(undefined)
})

describe('SettingsPage 首选认证方式回显（C-SP1/C-SP2/C-SP3）', () => {
  it('C-SP1：KV 无该项时默认选中配对码', async () => {
    const wrapper = await mountPage()
    const segments = authSegments(wrapper)
    expect(segments.length).toBe(2)
    expect(segments[0].attributes('aria-pressed')).toBe('true')
    expect(segments[1].attributes('aria-pressed')).toBe('false')
    expect(segments[0].text()).toBe('settings.authMethod.pairingCode')
    wrapper.unmount()
  })

  it('C-SP2：KV 为 biometric 时回显生物识别段', async () => {
    readAllSettings.mockResolvedValue({ 'mobile.preferredAuthMethod': 'biometric' })
    const wrapper = await mountPage()
    const segments = authSegments(wrapper)
    expect(segments[0].attributes('aria-pressed')).toBe('false')
    expect(segments[1].attributes('aria-pressed')).toBe('true')
    wrapper.unmount()
  })

  it('C-SP3：点生物识别段落库并翻转选中态', async () => {
    const wrapper = await mountPage()
    await authSegments(wrapper)[1].trigger('click')
    await nextTick()

    expect(writeSetting).toHaveBeenCalledWith('mobile.preferredAuthMethod', 'biometric')
    const segments = authSegments(wrapper)
    expect(segments[1].attributes('aria-pressed')).toBe('true')
    expect(segments[0].attributes('aria-pressed')).toBe('false')
    wrapper.unmount()
  })
})
