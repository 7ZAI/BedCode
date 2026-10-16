/**
 * 业务设置页 渲染与交互契约测试
 * （2026-10-15：修复「首选认证方式不回显」——分段按钮把**设置键**当成了选项值比较；
 *  2026-10-16：主色色板改为色块预览单选按钮 + 断言共享状态跨实例可见）
 *
 * 被测：`src/settings/SettingsPage.vue`（挂载真实组件 + 替身宿主 KV 桥）。
 * 替身边界：只 mock 宿主 KV 桥（getMobileApi 的 readAllSettings / writeSetting）。
 * 隔离：被测模块（useAppSettings）持有模块级共享状态，每个用例 `vi.resetModules()`
 * 重建模块，避免上一个用例的写入泄漏进下一个用例。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-SP1 默认回显 | KV 无 `mobile.preferredAuthMethod` | 按定义默认值 `pairing_code` 选中 | 「配对码」段 aria-pressed=true、生物段 false |
 * | C-SP2 落库回显 | KV 有 `mobile.preferredAuthMethod=biometric` | 选中项跟随 KV | 生物段 true、配对码段 false |
 * | C-SP3 点击落库 | 点「生物识别」段 | 归一后写 KV 并更新本地值 | writeSetting('mobile.preferredAuthMethod','biometric') + 选中态翻转 |
 * | C-SP4 色板渲染 | accentPalette 定义 swatchPreview | 5 个色块单选按钮，色值 token + 文字标签 + aria-pressed | 默认档选中，其余未选中 |
 * | C-SP5 点色块 | 点 ocean 档 | 落库 + 选中态翻转 + 共享状态跨实例立即可见 | 运行面同源读到 ocean |
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

let SettingsPage: typeof import('../SettingsPage.vue')['default']
let useAppSettings: (
  provided?: PluginContext,
) => ReturnType<typeof import('../useAppSettings')['useAppSettings']>

/** 最小插件上下文替身（i18n 回显 key；toast/confirm 记录调用） */
function makePluginContext() {
  return {
    id: 'com.bedcode.terminal-session',
    i18n: { t: (key: string) => key },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
    dialogs: { showToast: vi.fn(), showConfirm: vi.fn(async () => true) },
  } as unknown as PluginContext
}

/**
 * 认证方式分段按钮（按选项文案 key 定位）
 *
 * 不能用 `button[aria-pressed]` 全量取再按下标——页面同时有 2 个认证段 +
 * 5 个主色色块单选按钮，按下标定位会打到色板那一行去。
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function authSegments(wrapper: any) {
  const all = wrapper.findAll('button[aria-pressed]') as any[]
  const picked = all.filter((b) =>
    ['settings.authMethod.pairingCode', 'settings.authMethod.biometric'].includes(b.text()),
  )
  if (picked.length !== 2) {
    throw new Error(`认证方式段应为 2 个，实际 ${picked.length} 个：${all.map((b) => b.text()).join(' | ')}`)
  }
  return picked
}

/** 主色色板选项按钮（按 data-palette-option 定位） */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function paletteOptions(wrapper: any) {
  return wrapper.findAll('button[data-palette-option]') as any[]
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

beforeEach(async () => {
  vi.resetModules()
  ;({ default: SettingsPage } = await import('../SettingsPage.vue'))
  ;({ useAppSettings } = await import('../useAppSettings'))
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

describe('SettingsPage 主色色板色块预览（C-SP4/C-SP5，2026-10-16）', () => {
  it('C-SP4：渲染 5 个色块单选按钮，默认档选中，其余未选中不等宽挤压', async () => {
    const wrapper = await mountPage()
    const options = paletteOptions(wrapper)
    expect(options.length).toBe(5)

    // 每个选项 = 色块（色值 token 注入）+ 文字标签 + 选中态 aria-pressed（不只靠颜色传达）
    for (const opt of options) {
      const value = opt.attributes('data-palette-option') as string
      const dot = opt.find('span[aria-hidden="true"]')
      expect(dot.exists()).toBe(true)
      expect(dot.attributes('style')).toContain(`--mobile-palette-swatch-${value}`)
      expect(opt.text().length).toBeGreaterThan(0)
    }

    // 默认档（默认值）选中，其余未选中；勾选图标只在选中项内
    for (const opt of options) {
      const isDefault = opt.attributes('data-palette-option') === 'default'
      expect(opt.attributes('aria-pressed')).toBe(isDefault ? 'true' : 'false')
      expect(opt.find('svg').exists()).toBe(isDefault)
    }
    wrapper.unmount()
  })

  it('C-SP5：点 ocean 色块落库 + 选中态翻转 + 共享状态跨实例立即可见', async () => {
    const wrapper = await mountPage()

    const ocean = paletteOptions(wrapper).find(
      (o) => o.attributes('data-palette-option') === 'ocean',
    )!
    await ocean.trigger('click')
    await nextTick()

    expect(writeSetting).toHaveBeenCalledWith('mobile.accentPalette', 'ocean')

    const after = paletteOptions(wrapper)
    const oceanAfter = after.find((o) => o.attributes('data-palette-option') === 'ocean')!
    const defaultAfter = after.find((o) => o.attributes('data-palette-option') === 'default')!
    expect(oceanAfter.attributes('aria-pressed')).toBe('true')
    expect(defaultAfter.attributes('aria-pressed')).toBe('false')
    expect(oceanAfter.find('svg').exists()).toBe(true)
    expect(defaultAfter.find('svg').exists()).toBe(false)

    // 运行面（另一实例，如 AppRoot 的 data-palette 计算）无需重新 load 即可见新主色
    const appRootCtrl = useAppSettings(makePluginContext())
    expect(appRootCtrl.values.accentPalette).toBe('ocean')
    wrapper.unmount()
  })

  it('C-SP6：色板选择随 KV 回显（forest 落库时 forest 选中有勾）', async () => {
    readAllSettings.mockResolvedValue({ 'mobile.accentPalette': 'forest' })
    const wrapper = await mountPage()

    const forest = paletteOptions(wrapper).find(
      (o) => o.attributes('data-palette-option') === 'forest',
    )!
    const defaultOption = paletteOptions(wrapper).find(
      (o) => o.attributes('data-palette-option') === 'default',
    )!
    expect(forest.attributes('aria-pressed')).toBe('true')
    expect(forest.find('svg').exists()).toBe(true)
    expect(defaultOption.attributes('aria-pressed')).toBe('false')
    wrapper.unmount()
  })
})