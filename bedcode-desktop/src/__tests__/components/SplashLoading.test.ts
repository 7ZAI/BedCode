import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import SplashLoading from '@/components/SplashLoading.vue'

// mock Tauri 运行时 API：getVersion 返回固定版本，验证 footer 版本号动态渲染
vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn(() => Promise.resolve('9.9.9')),
}))

/** 测试用最小 i18n 实例：仅含 splash 相关 key */
function createTestI18n() {
  return createI18n({
    legacy: false,
    locale: 'zh-CN',
    fallbackLocale: 'zh-CN',
    messages: {
      'zh-CN': {
        desktop: {
          splash: {
            status: '正在启动…',
            tagline: '局域网远程终端工作台',
          },
        },
      },
    },
  })
}

function mountSplash(props: Record<string, unknown>, slots: Record<string, unknown> = {}) {
  return mount(SplashLoading, {
    props,
    slots,
    global: {
      plugins: [createTestI18n()],
      stubs: {
        Teleport: false,
        Transition: false,
      },
    },
    attachTo: document.body,
  })
}

describe('SplashLoading Component', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it('should not render overlay when visible is false', () => {
    mountSplash({ visible: false })
    expect(document.querySelector('.splash-root')).toBeNull()
  })

  it('should render fullscreen overlay with z-[100] when visible', () => {
    const wrapper = mountSplash({ visible: true })

    const overlay = document.querySelector('.splash-root')
    expect(overlay).toBeTruthy()
    expect(overlay?.classList.contains('z-[100]')).toBe(true)
    wrapper.unmount()
  })

  it('should render brand wordmark and boot command line', () => {
    const wrapper = mountSplash({ visible: true })

    const overlay = document.querySelector('.splash-root')!
    // 品牌名
    expect(overlay.textContent).toContain('BedCode')
    // 终端启动行：$ 提示符 + 命令（Teleport 到 body，用 document 查询）
    const typed = overlay.querySelector('.splash-typed')
    expect(typed?.textContent).toBe('bedcode')
    wrapper.unmount()
  })

  it('should fall back to i18n status when status prop is empty', () => {
    const wrapper = mountSplash({ visible: true })

    const overlay = document.querySelector('.splash-root')!
    expect(overlay.textContent).toContain('正在启动…')
    wrapper.unmount()
  })

  it('should display custom status prop over i18n default', () => {
    const wrapper = mountSplash({ visible: true, status: 'Custom status' })

    const overlay = document.querySelector('.splash-root')!
    expect(overlay.textContent).toContain('Custom status')
    expect(overlay.textContent).not.toContain('正在启动…')
    wrapper.unmount()
  })

  it('should hide progress bar by default and show it with width binding', () => {
    const hidden = mountSplash({ visible: true })
    expect(document.querySelector('.splash-progress')).toBeNull()
    hidden.unmount()

    const shown = mountSplash({ visible: true, showProgress: true, progress: 50 })
    const bar = document.querySelector<HTMLElement>('.splash-progress')
    expect(bar).toBeTruthy()
    expect(bar?.style.width).toBe('50%')
    shown.unmount()
  })

  it('should render runtime app version in footer when getVersion resolves', async () => {
    const wrapper = mountSplash({ visible: true })

    // 等待 onMounted 中的 getVersion() resolve
    await flushPromises()

    const overlay = document.querySelector('.splash-root')!
    expect(overlay.querySelector('.splash-footer')?.textContent).toContain('v9.9.9')
    expect(overlay.querySelector('.splash-footer')?.textContent).toContain('LAN remote terminal')
    wrapper.unmount()
  })

  it('should allow logo slot override', () => {
    const wrapper = mountSplash({ visible: true }, { logo: '<div class="custom-logo">X</div>' })

    expect(document.querySelector('.custom-logo')).toBeTruthy()
    // 默认品牌 glyph 不渲染
    expect(document.querySelector('.splash-logo')).toBeNull()
    wrapper.unmount()
  })
})
