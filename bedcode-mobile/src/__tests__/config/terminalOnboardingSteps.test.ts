/**
 * 终端新手引导步骤 i18n 完整性测试
 *
 * 防回归：组件引用的 titleKey/descKey/tryHintKey 必须在 zh-CN 与 en 两份
 * 语言文件中都存在（此前发生过步骤标题 key 漏加、真机渲染裸 key 的问题）。
 * key 定义在 config/terminalOnboardingSteps.ts（mobile.terminal.* 前缀在
 * 组件内拼接，此处按裸 key 断言）。
 */
import { describe, it, expect } from 'vitest'
import { TERMINAL_ONBOARDING_STEPS } from '@/config/terminalOnboardingSteps'
import zhCNMobile from '@/locales/zh-CN/mobile'
import enMobile from '@/locales/en/mobile'

const catalogs = { 'zh-CN': zhCNMobile, en: enMobile } as const

function resolve(catalog: Record<string, unknown>, key: string): unknown {
  return key.split('.').reduce<unknown>((node, part) => {
    if (node && typeof node === 'object') return (node as Record<string, unknown>)[part]
    return undefined
  }, catalog)
}

describe('terminal onboarding steps i18n completeness', () => {
  it('every step exposes non-empty required keys（targetSelector/titleKey/descKey）', () => {
    expect(TERMINAL_ONBOARDING_STEPS.length).toBeGreaterThanOrEqual(9)
    for (const step of TERMINAL_ONBOARDING_STEPS) {
      // 必填字段：非空字符串（toThrow 于空/缺 key 的步骤）
      expect(step.targetSelector, `${step.titleKey} targetSelector`).toBeTruthy()
      expect(step.titleKey, `titleKey`).toBeTruthy()
      expect(step.descKey, `descKey`).toBeTruthy()
      // 可选 tryHintKey：一旦声明必须是非空字符串（不允许空串占位）
      if (step.tryHintKey !== undefined) {
        expect(step.tryHintKey.length, `${step.titleKey} tryHintKey non-empty`).toBeGreaterThan(0)
      }
    }
  })

  for (const [locale, catalog] of Object.entries(catalogs)) {
    it(`resolves all step keys in ${locale} under mobile.terminal`, () => {
      for (const step of TERMINAL_ONBOARDING_STEPS) {
        const keys = [step.titleKey, step.descKey]
        // tryHintKey 可选：声明了才要求 i18n 可解析（不静默放行缺失 key）
        if (step.tryHintKey) keys.push(step.tryHintKey)
        for (const key of keys) {
          const value = resolve(catalog, `mobile.terminal.${key}`)
          expect(value, `${locale}: mobile.terminal.${key} 缺失`).toBeDefined()
          expect(
            typeof value === 'string' && value.length > 0,
            `${locale}: mobile.terminal.${key} 非空`,
          ).toBe(true)
        }
      }
    })
  }
})
