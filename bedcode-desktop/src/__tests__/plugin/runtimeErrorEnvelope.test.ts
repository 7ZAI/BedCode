/**
 * 插件运行时错误信封契约单测（票 03 / ADR 0030 决定 7、11）
 *
 * 本文件锁「可单测的纯契约」两件事：
 * 1. **注册表文案**：三个运行时码 + 降级码在 zh / en 都有文案，模板无未替换占位符，
 *    且文案里没有错误码本身（code = i18n key，但 key 不是给用户看的文本）。
 * 2. **退役锁**：随票 03 下线的 `desktop.plugin.*` 运行时文案键与
 *    `getErrorMessage` / `getDegradedMessage` 两个原文取值函数不得复活——
 *    行为测试证明不了「键已删除」（有键无键行为一致），只能扫描。
 *
 * 「事件 → toast」的端到端行为在集成测试 `src/__tests__/integration/plugin-error-envelope.test.ts`
 * （票 05 统一执行）。
 */
import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import i18n from '@/locales'

/** 票 03 落地的宿主域运行时码（= 事件信封 code，改码 = 破坏性变更） */
const REGISTRY_V0_PLUGIN_CODES = [
  'host.plugin.trap',
  'host.plugin.recovery-failed',
  'host.plugin.self-check-failed',
  'host.plugin.degraded',
] as const

/** 各码的插值参数名：与 Rust `runtime_error_envelope` / `self_check_envelope` 的 params 同名 */
const CODE_PARAMS: Record<string, string> = {
  'host.plugin.trap': 'name',
  'host.plugin.recovery-failed': 'name',
  'host.plugin.self-check-failed': 'plugin',
  'host.plugin.degraded': 'name',
}

describe('注册表 v0：插件运行时错误码文案', () => {
  it.each(REGISTRY_V0_PLUGIN_CODES)('%s 在 zh-CN 有文案，且模板已全部插值', (code) => {
    const key = `errors.${code}`
    expect(i18n.global.te(key), `${key} 缺 zh 文案`).toBe(true)
    const param = CODE_PARAMS[code]
    const text = i18n.global.t(key, { [param]: 'AI Chatbox' })
    expect(text).toContain('AI Chatbox')
    // 模板占位符必须被参数替换干净：残留 `{...}` = 插值名与 Rust params 不一致
    expect(text).not.toMatch(/\{[a-z_]+\}/)
  })

  it.each(REGISTRY_V0_PLUGIN_CODES)('%s 在 en 有同名文案（zh/en 同步）', (code) => {
    const key = `errors.${code}`
    i18n.global.locale.value = 'en'
    try {
      expect(i18n.global.te(key), `${key} 缺 en 文案`).toBe(true)
      const text = i18n.global.t(key, { [CODE_PARAMS[code]]: 'AI Chatbox' })
      expect(text).toContain('AI Chatbox')
      expect(text).not.toMatch(/\{[a-z_]+\}/)
    } finally {
      i18n.global.locale.value = 'zh-CN'
    }
  })

  it.each(REGISTRY_V0_PLUGIN_CODES)('%s 文案不出现错误码字面量（用户面不显示内部标识）', (code) => {
    for (const locale of ['zh-CN', 'en'] as const) {
      i18n.global.locale.value = locale
      const text = i18n.global.t(`errors.${code}`, { [CODE_PARAMS[code]]: 'AI Chatbox' })
      expect(text).not.toContain(code)
      expect(text).not.toContain('request_id')
    }
    i18n.global.locale.value = 'zh-CN'
  })

  it('降级码文案带应用名（列表 tooltip / 详情页降级段共用同一句）', () => {
    const text = i18n.global.t('errors.host.plugin.degraded', { name: '文件传输' })
    expect(text).toContain('文件传输')
  })
})

describe('退役锁：插件错误原文不得回到 UI 消费面', () => {
  /** 票 03 退役的键：曾把完整错误串插值进 toast / tooltip / 降级原因段落 */
  const RETIRED_I18N_KEYS = [
    'desktop.plugin.degradedReason',
    'desktop.plugin.selfCheckFailed',
    'desktop.plugin.runtimePanic',
    'desktop.plugin.runtimeTrap',
    'desktop.plugin.runtimeRecoveryFailed',
  ]

  const LOCALE_FILES = ['src/locales/zh-CN/desktop.ts', 'src/locales/en/desktop.ts']

  it.each(LOCALE_FILES)('%s 不再定义已退役的运行时文案键', (file) => {
    const source = readFileSync(file, 'utf-8')
    for (const key of RETIRED_I18N_KEYS) {
      const leaf = key.split('.').pop()!
      expect(source, `${file} 仍定义退役键 ${key}`).not.toMatch(
        new RegExp(`^\\s*${leaf}:`, 'm'),
      )
    }
  })

  it('contributionKinds 不再导出 state.error 取值函数', () => {
    const source = readFileSync('src/plugin/contributionKinds.ts', 'utf-8')
    expect(source).not.toMatch(/export function get(Error|Degraded)Message/)
    // 取值本身也一并退役：state.error 是宿主诊断事实，不进 UI 消费面
    expect(source).not.toMatch(/state\.error/)
  })

  const VIEW_FILES = ['src/views/PluginsView.vue', 'src/views/PluginDetailView.vue']

  it.each(VIEW_FILES)('%s 不再渲染插件错误原文', (file) => {
    const source = readFileSync(file, 'utf-8')
    expect(source).not.toMatch(/get(Error|Degraded)Message/)
    expect(source).not.toMatch(/state\.error/)
  })

  it('runtime-listeners 只经消费层出文案（无截断展示、无旧键直译）', () => {
    const source = readFileSync('src/plugin/runtime-listeners.ts', 'utf-8')
    // 旧实现把 panic 消息/回溯截断 120 字塞进 toast —— 硬不变量：详情不出产生方进程
    expect(source).not.toMatch(/\.slice\(0,\s*\d+\)/)
    expect(source).not.toMatch(/desktop\.plugin\.(selfCheckFailed|runtime\w+)/)
    expect(source).toContain('showUserError')
  })
})
