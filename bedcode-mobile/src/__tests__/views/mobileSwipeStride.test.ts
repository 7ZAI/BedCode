/**
 * MobileSwipeContainer 翻页步进宽度（translateX stride）回归测试
 *
 * 行为契约：translateX 每翻一页的位移量必须取「轨道实测宽度」，不能取 window.innerWidth。
 *
 * 两者只在「祖先链完全没有收窄内容区」时才相等（无 padding / border / 滚动条预留槽）。
 * 一旦页宽窄于视口，用 innerWidth 当步进会让第 N 页多推 (innerWidth - 页宽) × N：
 * 真机表现是页面左侧被裁掉同等像素、右侧留出空白带，看起来像整页错位。
 * 历史事故：全局 `* { scrollbar-gutter: stable }` 在真机 WebView 上给 overflow:hidden
 * 包装层也预留了滚动条槽（该 WebView 的 ::-webkit-scrollbar 是占位式而非 overlay），
 * 页宽 696 / 视口 712，第 4 页多推 48px——设置页标题与卡片图标被裁。
 *
 * 2026-10-02 后续：该全局规则已整体删除，只留 `.scrollbar-gutter-stable` 工具类按需写在
 * 页面内容滚动容器上。同一类死区当时在终端页放大成 9 层 × 8px = 69px，终端右侧露出一条
 * 32px 浅色空带。本用例记录的教训不变：**祖先链一旦被收窄，innerWidth 就不是可用的
 * 步进基准**，无论收窄来自什么规则。
 *
 * 正例：页宽 < 视口 → 按页宽步进（本次修复的行为）
 * 反例：页宽未布局（clientWidth = 0）→ 回退 innerWidth，避免出现 NaN / 0 位移
 * 边界：页宽 == 视口 → 与旧行为等价（无 gutter 的常规环境不回归）
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { reactive, nextTick } from 'vue'
import { mount } from '@vue/test-utils'

// 路由是响应式对象：组件 watch route.query.page，测试需能改写
const route = reactive({
  name: 'mobile-home',
  query: {} as Record<string, string>,
})

vi.mock('vue-router', () => ({
  useRoute: () => route,
  useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
}))
// 插件页签注册表：本用例只验内置页，桩成空数组避免真实插件注册态渗入
vi.mock('@/plugin/registry', () => ({
  getPluginRegistry: () => ({ navTabs: { value: [] } }),
}))
// 视图组件在本用例只作为挂载占位（mount 时 stub），但它们的**模块导入**会执行
// useMobileConnection 的模块级 init() → @tauri-apps/api/event 的 listen 在
// happy-dom 下访问 window.__TAURI_INTERNALS__.transformCallback 抛错（与
// setup.ts 同源的 Tauri 内核缺失）。桩掉 event 模块阻断该导入期副作用。
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}))

import MobileSwipeContainer from '@/components/MobileSwipeContainer.vue'

const VIEWPORT = 712
/** 真机实测页宽：视口 712 被两层 overflow:hidden 包装层的滚动条槽各收 8px */
const PAGE_WITH_GUTTER = 696

const originalInnerWidth = window.innerWidth
const originalClientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth')

function setViewportWidth(px: number): void {
  Object.defineProperty(window, 'innerWidth', { configurable: true, writable: true, value: px })
}

/** 把所有元素的 clientWidth 固定为 px（0 = 保持 happy-dom 默认，模拟「尚未布局」） */
function setClientWidth(px: number): void {
  if (px === 0) {
    if (originalClientWidth) Object.defineProperty(HTMLElement.prototype, 'clientWidth', originalClientWidth)
    else delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientWidth
    return
  }
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
    configurable: true,
    get(this: HTMLElement) {
      return this.classList.contains('swipe-track') ||
        this.classList.contains('swipe-page') ||
        this.classList.contains('swipe-container')
        ? px
        : 0
    },
  })
}

/** 从轨道 style 里取 translate3d 的 X 位移 */
function stride(wrapper: ReturnType<typeof mount>): number {
  const style = wrapper.find('.swipe-track').attributes('style') ?? ''
  const m = /translate3d\((-?[\d.]+)px/.exec(style)
  return m ? Number(m[1]) : Number.NaN
}

function mountContainer() {
  return mount(MobileSwipeContainer, {
    global: {
      stubs: {
        DevicesView: true,
        SessionsView: true,
        ToolboxView: true,
        SettingsView: true,
        PluginNavTabHost: true,
      },
    },
  })
}

describe('MobileSwipeContainer 翻页步进宽度', () => {
  beforeEach(() => {
    setViewportWidth(VIEWPORT)
    route.name = 'mobile-home'
    route.query = {}
  })

  afterEach(() => {
    setViewportWidth(originalInnerWidth)
    setClientWidth(0)
  })

  it('页宽窄于视口时按实测页宽步进，不用 innerWidth（滚动条槽回归正例）', async () => {
    route.query = { page: '3' }
    setClientWidth(PAGE_WITH_GUTTER)

    const wrapper = mountContainer()
    // translateX 在 onMounted 里写入，样式绑定需等一次渲染刷新才落到 DOM
    await nextTick()

    // 旧实现会算出 -3 * 712 = -2136（多推 48px，左侧裁切 + 右侧空白）
    expect(stride(wrapper)).toBe(-3 * PAGE_WITH_GUTTER)
    expect(stride(wrapper)).not.toBe(-3 * VIEWPORT)
  })

  it('页宽未布局（clientWidth = 0）时回退到 innerWidth，不产生 0 位移', async () => {
    route.query = { page: '2' }
    setClientWidth(0)

    const wrapper = mountContainer()
    await nextTick()

    expect(stride(wrapper)).toBe(-2 * VIEWPORT)
  })

  it('页宽等于视口时与旧行为等价（无滚动条槽的常规环境不回归）', async () => {
    route.query = { page: '3' }
    setClientWidth(VIEWPORT)

    const wrapper = mountContainer()
    await nextTick()

    expect(stride(wrapper)).toBe(-3 * VIEWPORT)
  })

  it('窗口尺寸变化后按新的实测页宽重新对齐', async () => {
    route.query = { page: '3' }
    setClientWidth(PAGE_WITH_GUTTER)

    const wrapper = mountContainer()
    await nextTick()
    expect(stride(wrapper)).toBe(-3 * PAGE_WITH_GUTTER)

    // 祖先收窄量变化（如键盘弹出 / 旋转后滚动条槽变化）后须按新页宽重算
    setClientWidth(600)
    window.dispatchEvent(new Event('resize'))
    await nextTick()

    expect(stride(wrapper)).toBe(-3 * 600)
  })

  it('首页位移恒为 0（页宽为 0 时也不产生 -0 / NaN）', async () => {
    route.query = { page: '0' }
    setClientWidth(PAGE_WITH_GUTTER)

    const wrapper = mountContainer()
    await nextTick()

    expect(stride(wrapper)).toBe(0)
  })
})
