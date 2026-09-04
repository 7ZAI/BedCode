import { describe, it, expect, vi } from 'vitest'
import { defineComponent, h, nextTick } from 'vue'
import type { PlatformInfo } from '@/composables/model'

// 通过 mock 让每个用例都能改写 platformInfo，并驱动 composable 在 setup 内执行
const mocks = vi.hoisted(() => {
  const { ref } = require('vue') as typeof import('vue')
  return {
    platformInfo: ref<PlatformInfo>({
      platform: 'windows',
      arch: 'x86_64',
      osVersion: 'test',
      osType: 'test',
      isDesktop: true,
      isMobile: false,
      isWindows: true,
      isMacos: false,
      isLinux: false,
      isAndroid: false,
      isIos: false,
    }),
  }
})

vi.mock('@/composables/usePlatform', () => ({
  usePlatform: () => ({ platformInfo: mocks.platformInfo }),
}))

// i18n 在测试环境里挂一个最小实例，绕过「必须在 setup 调用」限制
vi.mock('vue-i18n', async () => {
  const actual = await vi.importActual<typeof import('vue-i18n')>('vue-i18n')
  return {
    ...actual,
    useI18n: () => ({
      t: (key: string) => key,
      locale: { value: 'en' },
    }),
  }
})

import { useAvailableEnvironments } from '@/composables/useAvailableEnvironments'

function setPlatform(p: Partial<PlatformInfo>) {
  mocks.platformInfo.value = { ...mocks.platformInfo.value, ...p }
}

/**
 * 把 composable 包成一个组件实例返回，让 useI18n 之类的插件能在 setup 内调用
 */
function runComposable<T>(fn: () => T): Promise<T> {
  let captured: T | undefined
  const Comp = defineComponent({
    setup() {
      captured = fn()
      return () => h('div')
    },
  })
  // 触发 setup
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { mount } = require('@vue/test-utils') as typeof import('@vue/test-utils')
  const wrapper = mount(Comp)
  return Promise.resolve(captured!).then(async (v) => {
    await nextTick()
    wrapper.unmount()
    return v
  })
}

describe('useAvailableEnvironments', () => {
  it('windows 平台：白名单为 windows + wsl2', async () => {
    setPlatform({ platform: 'windows', isWindows: true, isLinux: false, isDesktop: true })

    const { availableValues, defaultEnvironment, environmentOptions, normalizeEnvironment } =
      await runComposable(() => useAvailableEnvironments())

    expect(availableValues.value).toEqual(['windows', 'wsl2'])
    expect(defaultEnvironment.value).toBe('windows')

    const opts = environmentOptions.value
    expect(opts.find((o) => o.value === 'windows')?.available).toBe(true)
    expect(opts.find((o) => o.value === 'wsl2')?.available).toBe(true)
    expect(opts.find((o) => o.value === 'linux')?.available).toBe(false)

    expect(normalizeEnvironment('wsl2')).toBe('wsl2')
    expect(normalizeEnvironment('windows')).toBe('windows')
    expect(normalizeEnvironment('linux')).toBe('windows')
  })

  it('linux 平台：白名单为 linux', async () => {
    setPlatform({ platform: 'linux', isWindows: false, isLinux: true, isDesktop: true })

    const { availableValues, defaultEnvironment, environmentOptions, normalizeEnvironment } =
      await runComposable(() => useAvailableEnvironments())

    expect(availableValues.value).toEqual(['linux'])
    expect(defaultEnvironment.value).toBe('linux')

    const opts = environmentOptions.value
    expect(opts.find((o) => o.value === 'linux')?.available).toBe(true)
    expect(opts.find((o) => o.value === 'windows')?.available).toBe(false)
    expect(opts.find((o) => o.value === 'wsl2')?.available).toBe(false)

    expect(normalizeEnvironment('windows')).toBe('linux')
    expect(normalizeEnvironment('wsl2')).toBe('linux')
    expect(normalizeEnvironment('linux')).toBe('linux')
    expect(normalizeEnvironment(null)).toBe('linux')
    expect(normalizeEnvironment('unknown')).toBe('linux')
  })

  it('macOS / 未识别平台：白名单为空，defaultEnvironment 兜底为 windows', async () => {
    setPlatform({
      platform: 'macos',
      isWindows: false,
      isLinux: false,
      isMacos: true,
      isDesktop: true,
    })

    const { availableValues, defaultEnvironment } = await runComposable(() =>
      useAvailableEnvironments(),
    )

    expect(availableValues.value).toEqual([])
    expect(defaultEnvironment.value).toBe('windows')
  })
})
