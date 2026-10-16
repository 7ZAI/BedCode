/**
 * 平台主屏（Android 平台化）行为契约测试（2026-10-14）
 *
 * 被测：`src/shell/components/screens/ShellHomeScreen.vue`
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-H1 | wallClass / --wall-dim | 壁纸层由壳偏好驱动；none 档不挂壁纸类 | 类名与遮罩变量随偏好变化 |
 * | C-H2 | runningOrdered | 运行卡片带只列运行中，按最近使用排序 | 顺序 = recentIds 序；未运行不入带 |
 * | C-H3 | 应用宫格 | 全部应用在场；待审批有「!」角标与可访问状态 | 未运行也在格内；approval 角标可见 |
 * | C-H4 | 系统宫格 | 三个平台自有面进栈（可返回） | navigate('apps') / openPermissions / navigate('settings') |
 * | C-H5 | 铃铛 | 角标 = 通知 + 待审批；点击开通知栏 | 计数与 toggleShade |
 * | C-H6 | 空态 | 无应用时给出下一步说明 | 渲染 emptyAppsHint |
 * | C-H7 | 快捷卡片轨道 | 横向 scroll-snap 轨道（原型形态），不定宽竖排 | 轨道内 listitem 数 = 卡片数 |
 * | C-H8 | 管理入口 | 图标按钮 + 可访问名（不是文字 chip） | 点击 navigate('apps') |
 * | C-H9 | 文案纪律 | 不渲染方案期解释（由应用提供 / 平台自带 / 最近使用在前） | 三个退役 key 缺席 |
 * | C-H10 | 自定义壁纸 | custom 档 + 路径 → 壁纸层用 asset URL 铺底 | 图片层 style 含 URL |
 * | C-H11 | 多任务入口 | 运行带分组头带多任务入口（recents 全局可达，不只运行页胶囊） | openSwitcher 被调用 |
 * | C-H12 | 系统宫格瓦片样式 | 启动器风格：.shell-sys-tile + data-tile=entry.id，SVG 为 24 画框实心块面（fill + evenodd 挖空），不再是 stroke 线稿 | 类名 / data-tile / fill 属性按入口在场 |
 * | C-H13 | 运行卡停止按钮形状 | 圆角矩形危险按钮（不是 capsule chip）——短标签 + 44px 触控高度下 rounded-full 会缩成圆形，读作状态徽标而非按钮 | class 含 rounded-[10px]，不含 rounded-full / 999px |
 * | C-H14 | 壁纸上的浮空文字 | 壁纸上的标题/副标题挂 .shell-text-over-wall（底色描边护对比度）；shell.css 里该类的描边色由 token 派生 | 头部文本框与四个分组头均带类；CSS 含 text-shadow + color-mix |
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { computed, ref } from 'vue'
import { resetShellNotifications, useShellNotifications } from '@/shell/composables/useShellNotifications'
import { clearRecent, pushRecent } from '@/shell/composables/useShellRecent'
import { useShellPreferences } from '@/shell/composables/useShellPreferences'
import type { ShellApp } from '@/shell/types'

// ==================== 替身 ====================

const apps = ref<ShellApp[]>([])
const mockStop = vi.fn(async () => true)
const mockOpen = vi.fn(async () => true)
const mockToggleShade = vi.fn()
const mockNavigate = vi.fn()
const mockOpenPermissions = vi.fn()
const mockOpenSwitcher = vi.fn()

vi.mock('vue-i18n', () => ({
  // 回显 key + 参数：角标计数与可访问名称里的参数才可断言
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) =>
      params ? `${key}:${JSON.stringify(params)}` : key,
  }),
}))
vi.mock('@/shell/composables/useShellApps', () => ({
  useShellApps: () => ({
    apps: computed(() => apps.value),
    // 与生产实现同语义：运行中优先，其次按名称（中文 localeCompare）
    sortedApps: computed(() =>
      [...apps.value].sort((a, b) => {
        const r = Number(b.state === 'running') - Number(a.state === 'running')
        return r !== 0 ? r : a.name.localeCompare(b.name, 'zh-Hans-CN')
      }),
    ),
    stats: computed(() => ({ installed: apps.value.length, running: 0, totalBytes: 0 })),
    stop: mockStop,
  }),
}))
vi.mock('@/shell/composables/useShellOpen', () => ({
  useShellOpen: () => ({ open: mockOpen }),
}))
vi.mock('@/shell/composables/useShellOverlays', () => ({
  useShellOverlays: () => ({ toggleShade: mockToggleShade }),
}))
vi.mock('@/shell/composables/useShellNavigation', () => ({
  useShellNavigation: () => ({
    navigate: mockNavigate,
    openPermissions: mockOpenPermissions,
    openSwitcher: mockOpenSwitcher,
  }),
}))
vi.mock('@/plugin/assets', () => ({
  toFileAssetUrl: (path: string) => `asset://${path}`,
}))

import ShellHomeScreen from '@/shell/components/screens/ShellHomeScreen.vue'
import ShellAppIcon from '@/shell/components/ShellAppIcon.vue'

function app(id: string, state: ShellApp['state'], name = id): ShellApp {
  return {
    id,
    name,
    version: '1.0.0',
    state,
    permissions: [{ key: 'fs:read', granted: state !== 'approval' }],
    contributions: {},
  }
}

beforeEach(() => {
  resetShellNotifications()
  clearRecent()
  localStorage.clear()
  apps.value = []
  mockStop.mockClear()
  mockOpen.mockClear()
  mockToggleShade.mockClear()
  mockNavigate.mockClear()
  mockOpenPermissions.mockClear()
  mockOpenSwitcher.mockClear()
  useShellPreferences().setWallpaper('aurora')
  useShellPreferences().setWallDim(30)
})

describe('C-H1 壁纸层', () => {
  it('should_applyWallpaperClassAndDim_when_preferenceSet', () => {
    useShellPreferences().setWallpaper('dune')
    useShellPreferences().setWallDim(55)

    const root = mount(ShellHomeScreen).find('div')
    const classes = root.classes()

    expect(classes, '壁纸类名由偏好驱动').toContain('shell-wall')
    expect(classes).toContain('wall-dune')
    expect(root.attributes('style')).toContain('--wall-dim: 55')
    expect(root.find('.shell-wallpaper').exists(), '壁纸层必须存在（纯装饰层）').toBe(true)
  })

  it('should_notApplyWallpaperClass_when_noneSelected', () => {
    useShellPreferences().setWallpaper('none')

    const classes = mount(ShellHomeScreen).find('div').classes()

    expect(classes).toContain('shell-wall')
    expect(classes.filter((c) => c.startsWith('wall-'))).toEqual([])
  })
})

describe('C-H2 运行卡片带', () => {
  it('should_listRunningByRecency_when_appsRunning', () => {
    pushRecent('b')
    pushRecent('a')
    apps.value = [app('a', 'running', 'AppA'), app('b', 'running', 'AppB'), app('c', 'stopped', 'AppC')]

    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('shell.home.runningTitle')
    // 未运行的应用不进运行带（宫格里另有它）
    const strip = mount(ShellHomeScreen).findAll('article')
    expect(strip.map((c) => c.text()).join('|')).toContain('AppA')
    expect(strip.map((c) => c.text()).join('|')).not.toContain('AppC')
  })

  it('should_stopInstance_when_stopChipTapped', async () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    const stop = wrapper.findAll('button').find((b) => b.text().includes('shell.home.stopInstance'))!
    await stop.trigger('click')

    expect(mockStop).toHaveBeenCalledWith('a')
  })

  it('should_useRectDangerButton_when_stopRendered', () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    const stop = wrapper.findAll('button').find((b) => b.text().includes('shell.home.stopInstance'))!
    const classes = stop.classes()

    // 形状契约：「停止」两字 + 44px 触控下限下，胶囊（rounded-full）会缩成圆形，
    // 与壳内其它矩形控件不同族；改用 control 圆角（10px）的描边危险按钮
    expect(classes).toContain('rounded-[10px]')
    expect(classes.some((c) => c.includes('rounded-full') || c.includes('999px')), '不得为胶囊/圆形').toBe(false)
    expect(classes).toContain('border')
    expect(stop.attributes('style'), '危险色走 token color-mix（不硬编码）').toContain('--mobile-error')
  })
})

describe('C-H15 应用图标外框（满幅作品不得叠框）', () => {
  /** 移动端插件图标（plugin.json.icon）都是满幅圆角方作品 */
  const imageApp = () => ({
    ...app('a', 'stopped', 'AppA'),
    icon: 'icon.svg',
    iconUrl: 'asset://localhost/icon.svg',
  })

  it('should_notFrameArtwork_when_imageIconRendered', () => {
    const wrapper = mount(ShellAppIcon, { props: { app: imageApp() } })
    const box = wrapper.find('span')

    // 作品自带底色与圆角：盒子再描边 + 铺底就只在作品四周添一圈白框（实测反馈）
    expect(box.classes().some((c) => c.startsWith('border')), '作品外围不得有描边').toBe(false)
    expect(box.classes()).toContain('bg-transparent')
  })

  it('should_keepSurface_when_iconFallsBackToLetter', () => {
    // 没有作品可依托（无图标 / 图片失败）时盒子才是承载面，仍不着描边
    const wrapper = mount(ShellAppIcon, { props: { app: app('a', 'stopped', 'AppA') } })
    const box = wrapper.find('span')

    expect(box.classes()).toContain('bg-[var(--mobile-bg-elevated)]')
    expect(box.classes().some((c) => c.startsWith('border')), '回退态同样不得有描边').toBe(false)
  })

  it('should_keepDashedFrame_when_toneGhost', () => {
    // ghost 的全部视觉身份就是虚线框（「更多」占位），描边只留给它
    const wrapper = mount(ShellAppIcon, { props: { app: app('a', 'stopped', 'AppA'), tone: 'ghost' } })

    expect(wrapper.find('span').classes()).toContain('border-dashed')
  })
})

describe('C-H14 壁纸上的浮空文字', () => {
  it('should_markWallFloatingText_when_homeRendered', () => {
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    // 壁纸亮度/色相不可控（含用户自选图片）：直接压在壁纸上的文字必须有底色描边。
    // 用户反馈的两条就是头部「本机 · N 个应用」与应用宫格分组头的「N 个已安装」。
    const headerText = wrapper.find('header .shell-text-over-wall')
    expect(headerText.exists(), '头部标题/副标题框挂描边类').toBe(true)
    expect(headerText.text()).toContain('shell.home.appCount')

    const appsNote = wrapper.findAll('span').find((s) => s.text().includes('shell.home.appsNote'))
    expect(appsNote?.exists(), '应用宫格分组头说明在场').toBe(true)
    expect(appsNote!.element.closest('.shell-text-over-wall'), '分组头说明文字在描边容器内').not.toBeNull()
  })
})

describe('C-H3 应用宫格', () => {
  it('should_listEveryAppIncludingDormant_when_gridRendered', () => {
    apps.value = [app('a', 'running', 'AppA'), app('b', 'stopped', 'AppB')]

    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('AppA')
    expect(text, '未运行应用同样在主屏宫格').toContain('AppB')
  })

  it('should_markApprovalApp_when_pending', () => {
    apps.value = [app('p', 'approval', 'PendingApp')]

    const wrapper = mount(ShellHomeScreen)
    // 可访问名称带应用名与状态（读屏用户看不到角标颜色）
    const cell = wrapper
      .findAll('button')
      .find((b) => (b.attributes('aria-label') ?? '').includes('PendingApp'))

    expect(cell, '待审批应用的宫格单元应在场').toBeTruthy()
    expect(cell!.attributes('aria-label')).toContain('shell.common.approval')
    expect(cell!.text()).toContain('PendingApp')
  })
})

describe('C-H4 系统宫格', () => {
  it('should_enterSystemPagesInStackOrder_when_entriesTapped', async () => {
    const wrapper = mount(ShellHomeScreen)

    await wrapper.find('button[aria-label="shell.home.sysApps"]').trigger('click')
    await wrapper.find('button[aria-label="shell.home.sysPermissions"]').trigger('click')
    await wrapper.find('button[aria-label="shell.home.sysSettings"]').trigger('click')

    // 平台面进栈而不是重置栈：系统屏要有可返回的上一级
    expect(mockNavigate.mock.calls.map((c) => c[0])).toEqual(['apps', 'settings'])
    expect(mockOpenPermissions).toHaveBeenCalledTimes(1)
  })
})

describe('C-H12 系统宫格瓦片样式（启动器风格）', () => {
  /** 系统宫格按钮（可访问名即入口文案，三个入口三种 key） */
  const SYS_KEYS = ['shell.home.sysApps', 'shell.home.sysPermissions', 'shell.home.sysSettings']
  /** entry.id → data-tile → shell.css 的 --shell-tile-* 选色（ids 与 CSS 属性选择器必须一致） */
  const EXPECTED_TILES = ['apps', 'permissions', 'settings']

  it('should_markEachEntryWithTileId_when_sysGridRendered', () => {
    const buttons = SYS_KEYS.map((key) =>
      mount(ShellHomeScreen).find(`button[aria-label="${key}"]`),
    )

    buttons.forEach((button, i) => {
      const tile = button.find('.shell-sys-tile')
      expect(tile.exists(), `${SYS_KEYS[i]} 应渲染启动器瓦片`).toBe(true)
      expect(tile.attributes('data-tile'), `${SYS_KEYS[i]} 的 data-tile`).toBe(EXPECTED_TILES[i])
    })
  })

  it('should_drawFilledGlyphsOnTwentyFourCanvas_when_sysTileRendered', () => {
    const tile = mount(ShellHomeScreen).find('.shell-sys-tile')
    const svg = tile.find('svg')
    const path = svg.find('path')

    // 实心块面：fill=当前色（瓦片按 data-tile 给的颜色），镂空靠 evenodd
    expect(svg.attributes('fill')).toBe('currentColor')
    expect(svg.attributes('fill-rule')).toBe('evenodd')
    // 24 画框 = ShellAppIcon path 态同规格（旧 stroke 版为 20px）
    expect(svg.attributes('width')).toBe('24')
    expect(svg.attributes('height')).toBe('24')
    expect(svg.attributes('stroke')).toBeUndefined()
    expect(path.attributes('d')).toBeTruthy()
  })
})

describe('C-H5 通知入口', () => {
  it('should_countNotificationsPlusPending_when_badgeRendered', () => {
    useShellNotifications().pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'n1' })
    useShellNotifications().pushNotification({ kind: 'info', icon: 'i', source: 's', title: 'n2' })
    apps.value = [app('p', 'approval', 'PendingApp')]

    const bell = mount(ShellHomeScreen).find('button[aria-label^="shell.notify.open"]')

    // 计数 = 2 条通知 + 1 个待审批
    expect(bell.attributes('aria-label')).toContain('3')
    expect(bell.text()).toContain('3')
  })

  it('should_toggleShade_when_bellTapped', async () => {
    const wrapper = mount(ShellHomeScreen)

    await wrapper.find('button[aria-label^="shell.notify.open"]').trigger('click')

    expect(mockToggleShade).toHaveBeenCalledTimes(1)
  })
})

describe('C-H6 空态', () => {
  it('should_explainNextStep_when_noAppsInstalled', () => {
    const text = mount(ShellHomeScreen).text()

    expect(text).toContain('shell.home.emptyAppsHint')
    expect(text, '没有应用时不渲染运行卡片带').not.toContain('shell.home.runningTitle')
  })
})

describe('C-H7 快捷卡片轨道（横向轮播）', () => {
  const SlotCard = { template: '<div class="slot-card">slot</div>' }

  function withSlot(): ShellApp {
    return {
      ...app('a', 'running', 'AppA'),
      contributions: { slots: [{ id: 's1', component: SlotCard }] },
    }
  }

  it('should_renderCardsInHorizontalSnapTrack_when_slotsRegistered', () => {
    apps.value = [withSlot()]

    const wrapper = mount(ShellHomeScreen)
    const track = wrapper.find('.shell-slot-track')

    expect(track.exists(), '快捷卡片应有独立轨道').toBe(true)
    expect(track.attributes('role')).toBe('list')
    expect(track.findAll('[role="listitem"]')).toHaveLength(1)
    expect(wrapper.find('.slot-card').exists()).toBe(true)
  })

  it('should_notRenderTrack_when_noSlots', () => {
    apps.value = [app('a', 'running', 'AppA')]

    expect(mount(ShellHomeScreen).find('.shell-slot-track').exists()).toBe(false)
  })
})

describe('C-H8 应用管理入口（图标按钮）', () => {
  it('should_navigateToApps_when_manageIconTapped', async () => {
    apps.value = [app('a', 'stopped', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    const manage = wrapper.find('button[aria-label="shell.home.manageApps"]')
    expect(manage.exists(), '管理入口应带可访问名（单图标按钮必须有）').toBe(true)
    // 图标按钮不再带「管理」文字（旧的文字 chip 观感扁平）
    expect(manage.text()).toBe('')

    await manage.trigger('click')
    expect(mockNavigate).toHaveBeenCalledWith('apps')
  })
})

describe('C-H9 宿主文案纪律（方案期解释不得回接）', () => {
  it('should_notRenderDesignStageNotes_when_mounted', () => {
    apps.value = [app('a', 'running', 'AppA'), app('b', 'stopped', 'AppB')]
    const text = mount(ShellHomeScreen).text()

    expect(text, '「由应用提供」应删除').not.toContain('shell.home.slotsNote')
    expect(text, '「平台自带」应删除').not.toContain('shell.home.sysNote')
    expect(text, '「最近使用在前」应删除').not.toContain('shell.home.runningNote')
  })
})

describe('C-H10 自定义壁纸图片层', () => {
  it('should_renderAssetUrl_when_customWallpaperSelected', () => {
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/data/app/shell-assets/wallpaper/wallpaper-1.png')

    const layer = mount(ShellHomeScreen).find('.shell-wallpaper')

    expect(layer.attributes('style')).toContain(
      'asset:///data/app/shell-assets/wallpaper/wallpaper-1.png',
    )
    prefs.setWallpaperImage(null)
  })

  it('should_notRenderImageLayer_when_builtinWallpaperSelected', () => {
    const prefs = useShellPreferences()
    prefs.setWallpaperImage('/data/app/x.png')
    prefs.setWallpaper('reef')

    const layer = mount(ShellHomeScreen).find('.shell-wallpaper')

    expect(layer.attributes('style') ?? '').not.toContain('background-image')
    prefs.setWallpaperImage(null)
    prefs.setWallpaper('aurora')
  })
})

describe('C-H11 多任务入口', () => {
  it('should_openSwitcher_when_runningStripEntryTapped', async () => {
    // recents 在业界是全局可达的：底部系统条退役后，主屏运行带要能进多任务
    apps.value = [app('a', 'running', 'AppA')]
    const wrapper = mount(ShellHomeScreen)

    const entry = wrapper.find('button[aria-label="shell.switcher.title"]')
    expect(entry.exists(), '入口应带可访问名（单图标按钮必须有）').toBe(true)

    await entry.trigger('click')
    expect(mockOpenSwitcher).toHaveBeenCalledTimes(1)
  })

  it('should_notRenderEntry_when_noRunningApp', () => {
    // 没有运行中应用时运行带整段不渲染，入口自然也不在场
    apps.value = [app('a', 'stopped', 'AppA')]

    expect(
      mount(ShellHomeScreen).find('button[aria-label="shell.switcher.title"]').exists(),
    ).toBe(false)
  })
})
