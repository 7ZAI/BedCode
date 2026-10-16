/**
 * 壳应用动作（审批 / 生命周期通知 / 待审批路由）行为契约测试（2026-10-14）
 *
 * 被测：`src/shell/composables/useShellApps.ts`（approveApp / launch / stop / install / remove）
 *       `src/shell/composables/useShellOpen.ts`（待审批进详情，不尝试启动）
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-A1 | approveApp 正路 | 调数据源审批 + 刷新 | 返回 true，审批记录落到数据源 |
 * | C-A2 | approveApp 反例 | 数据源不支持审批 | 返回 false 且不抛（UI 据此不渲染入口） |
 * | C-A3 | approveApp 异常 | 后端拒绝 | 返回 false + lastError 暴露原因 |
 * | C-A4 | launch 通知 | 未运行 → 启动成功推「已启动」 | 通知标题带应用名 |
 * | C-A5 | launch 通知去噪 | 已运行再启动（重挂前端模块） | 不重复推「已启动」 |
 * | C-A6 | launch 失败 | 后端拒绝 | 状态置 error + 推「启动失败」并带后端原因 |
 * | C-A7 | stop 通知 | 停止成功 | 推「已停止」 |
 * | C-A8 | install 通知 | 返回安装 id / 用户取消（null） | 有 id 才推「已安装」 |
 * | C-A9 | remove 通知 | 卸载成功 | 推「已卸载」（应用名取自卸载前快照） |
 * | C-A10 | stop 摘运行屏 | 停用成功且该应用运行屏在栈里 | 回落到该屏下面一屏（不留空态） |
 * | C-A11 | stop 不误伤 | 停用的是别的应用 / 停用失败 | 栈不动（当前运行屏仍可用） |
 * | C-A12 | remove 摘运行屏 | 卸载成功且该应用运行屏在栈里 | 回落到该屏下面一屏 |
 * | C-O1 | openShellApp 分支 | 待审批应用 | 进详情页，不调 launch |
 * | C-O2 | openShellApp 正路 | 可启动应用 | launch + 进运行面 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { getShellRegistry } from '@/shell/registry'
import { resetShellNotifications, useShellNotifications } from '@/shell/composables/useShellNotifications'
import { useShellApps } from '@/shell/composables/useShellApps'
import { openShellApp, resetShellNavigation, useShellNavigation } from '@/shell/index'
import type { ShellApp, ShellAppSource } from '@/shell/types'

const SOURCE_ID = 'app-actions-source'

vi.mock('@/locales', () => ({
  default: {
    global: {
      t: (key: string, params?: Record<string, unknown>) =>
        params ? `${key}:${JSON.stringify(params)}` : key,
    },
  },
}))

const shell = getShellRegistry()
const notifications = useShellNotifications()

/** 数据源行为由各用例定制（默认全部成功） */
const source: {
  approve: ReturnType<typeof vi.fn>
  launch: ReturnType<typeof vi.fn>
  stop: ReturnType<typeof vi.fn>
  install: ReturnType<typeof vi.fn>
  remove: ReturnType<typeof vi.fn>
} = {
  approve: vi.fn(async () => {}),
  launch: vi.fn(async () => {}),
  stop: vi.fn(async () => {}),
  install: vi.fn(async () => 'com.test.installed'),
  remove: vi.fn(async () => true),
}

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

function registeredSource(): ShellAppSource {
  return {
    id: SOURCE_ID,
    list: async () => [],
    launch: (...args: unknown[]) => source.launch(...(args as [string])) as Promise<void>,
    stop: (...args: unknown[]) => source.stop(...(args as [string])) as Promise<void>,
    approve: (...args: unknown[]) => source.approve(...(args as [string])) as Promise<void>,
    installFromLocalPackage: () =>
      source.install() as unknown as Promise<string | null>,
    remove: (...args: unknown[]) => source.remove(...(args as [string])) as Promise<boolean>,
  }
}

beforeEach(() => {
  resetShellNotifications()
  resetShellNavigation()
  localStorage.clear()
  shell.registerSource(registeredSource())
  for (const fn of Object.values(source)) fn.mockClear()
  source.approve.mockImplementation(async () => {})
  source.launch.mockImplementation(async () => {})
  source.stop.mockImplementation(async () => {})
  source.install.mockImplementation(async () => 'com.test.installed')
  source.remove.mockImplementation(async () => true)
})

afterEach(() => {
  shell.clearSource(SOURCE_ID)
})

describe('C-A1..C-A3 审批动作', () => {
  it('should_callSourceApproveAndRefresh_when_approving', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.p', 'approval', 'PendingApp')])

    const ok = await useShellApps().approveApp('com.test.p')

    expect(ok).toBe(true)
    expect(source.approve).toHaveBeenCalledWith('com.test.p')
    expect(
      notifications.notifications.value.some((n) => n.title.includes('shell.notify.app.approved.title')),
      '批准是用户可见事件，应落通知',
    ).toBe(true)
  })

  it('should_returnFalse_when_sourceCannotApprove', async () => {
    // 无审批能力的数据源：UI 据此不渲染入口，而不是点了没反应
    shell.registerSource({ ...registeredSource(), approve: undefined, id: 'no-approve-source' })
    shell.upsertApps('no-approve-source', [app('com.test.np', 'approval', 'NoApprove')])

    const ok = await useShellApps().approveApp('com.test.np')

    expect(ok).toBe(false)
    expect(source.approve).not.toHaveBeenCalled()
    shell.clearSource('no-approve-source')
  })

  it('should_exposeReason_when_backendRejectsApproval', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.p', 'approval', 'PendingApp')])
    source.approve.mockRejectedValueOnce(new Error('hash mismatch'))

    const appsApi = useShellApps()
    const ok = await appsApi.approveApp('com.test.p')

    expect(ok).toBe(false)
    expect(appsApi.lastError.value).toContain('hash mismatch')
  })
})

describe('C-A4..C-A7 启动 / 停止通知', () => {
  it('should_notifyStarted_when_appWasNotRunning', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])

    const ok = await useShellApps().launch('com.test.a')

    expect(ok).toBe(true)
    const titles = notifications.notifications.value.map((n) => n.title)
    expect(titles.join('|')).toContain('shell.notify.app.started.title')
    expect(titles.join('|'), '通知标题要带应用名').toContain('AppA')
  })

  it('should_notDuplicateStartedNotification_when_appAlreadyRunning', async () => {
    // 壳允许重复点已运行的应用（用于重挂前端模块）：那不是「又启动了一次」
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'running', 'AppA')])

    await useShellApps().launch('com.test.a')

    expect(source.launch, '数据源仍要跑一遍（幂等）').toHaveBeenCalled()
    expect(notifications.notifications.value).toHaveLength(0)
  })

  it('should_markErrorAndNotify_when_launchFails', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])
    source.launch.mockRejectedValueOnce(new Error('approval required'))

    const appsApi = useShellApps()
    const ok = await appsApi.launch('com.test.a')

    expect(ok).toBe(false)
    expect(appsApi.getApp('com.test.a')?.state).toBe('error')
    const failed = notifications.notifications.value.find((n) =>
      n.title.includes('shell.notify.app.failed.title'),
    )
    expect(failed, '启动失败必须可见（通知栏 + 列表状态）').toBeTruthy()
    expect(failed!.body, '失败原因来自后端，原样呈现').toContain('approval required')
  })

  it('should_notifyStopped_when_stopSucceeds', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'running', 'AppA')])

    const ok = await useShellApps().stop('com.test.a')

    expect(ok).toBe(true)
    expect(
      notifications.notifications.value.some((n) => n.title.includes('shell.notify.app.stopped.title')),
    ).toBe(true)
  })
})

describe('C-A8/C-A9 安装 / 卸载通知', () => {
  it('should_notifyInstalledWithAppName_when_installReturnsId', async () => {
    // 安装成功后清单里才有它：通知名要取刷新后的注册表事实
    shell.registerSource({
      ...registeredSource(),
      id: 'install-source',
      list: async () => [app('com.test.installed', 'approval', 'FreshApp')],
    })
    source.install.mockImplementationOnce(async () => 'com.test.installed')

    const ok = await useShellApps().installFromLocalPackage()

    expect(ok).toBe('com.test.installed')
    const installed = notifications.notifications.value.find((n) =>
      n.title.includes('shell.notify.app.installed.title'),
    )
    expect(installed).toBeTruthy()
    expect(installed!.title).toContain('FreshApp')
    shell.clearSource('install-source')
  })

  it('should_notNotify_when_userCancelsPicker', async () => {
    source.install.mockImplementationOnce(async () => null)

    const ok = await useShellApps().installFromLocalPackage()

    expect(ok).toBeNull()
    expect(notifications.notifications.value, '取消不是事件').toHaveLength(0)
  })

  it('should_notifyUninstalled_when_removeSucceeds', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])

    const ok = await useShellApps().removeApp('com.test.a')

    expect(ok).toBe(true)
    const removed = notifications.notifications.value.find((n) =>
      n.title.includes('shell.notify.app.uninstalled.title'),
    )
    expect(removed, '卸载后注册表已无该应用：名字取卸载前快照').toBeTruthy()
    expect(removed!.title).toContain('AppA')
  })
})

describe('C-A10..C-A12 停用 / 卸载后离开运行面', () => {
  it('should_leaveRunScreen_when_stoppingAppShownInRunScreen', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'running', 'AppA')])
    const nav = useShellNavigation()
    nav.openApp('com.test.a')

    const ok = await useShellApps().stop('com.test.a')

    expect(ok).toBe(true)
    // 运行面注册随停用被 dispose：留在运行屏只会看到「应用暂无可用界面」空态
    expect(nav.current.value.id).toBe('home')
  })

  it('should_leaveRunScreenAndOverlays_when_stoppingFromDetailScreen', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'running', 'AppA')])
    const nav = useShellNavigation()
    nav.openApp('com.test.a')
    nav.openDetail('com.test.a')

    await useShellApps().stop('com.test.a')

    expect(nav.current.value.id, '详情页盖在运行屏上：一起摘掉，别回落到已失效的运行面').toBe('home')
  })

  it('should_keepRunScreen_when_stoppingAnotherApp', async () => {
    shell.upsertApps(SOURCE_ID, [
      app('com.test.a', 'running', 'AppA'),
      app('com.test.b', 'running', 'AppB'),
    ])
    const nav = useShellNavigation()
    nav.openApp('com.test.a')

    await useShellApps().stop('com.test.b')

    expect(nav.current.value.id, '停用的是别的应用：当前运行屏不动').toBe('app-run')
    expect(nav.params.value.appId).toBe('com.test.a')
  })

  it('should_keepRunScreen_when_stopFails', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'running', 'AppA')])
    const nav = useShellNavigation()
    nav.openApp('com.test.a')
    source.stop.mockRejectedValueOnce(new Error('busy'))

    const ok = await useShellApps().stop('com.test.a')

    expect(ok).toBe(false)
    expect(nav.current.value.id, '停用失败：运行面多半还在，留用户看错误态').toBe('app-run')
  })

  it('should_leaveRunScreen_when_removingAppShownInRunScreen', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])
    const nav = useShellNavigation()
    nav.openApp('com.test.a')

    const ok = await useShellApps().removeApp('com.test.a')

    expect(ok).toBe(true)
    expect(nav.current.value.id, '应用卸载后再返回只会剩「应用不存在」提示').toBe('home')
  })
})

describe('C-O1/C-O2 打开应用路由', () => {
  it('should_openDetailInsteadOfLaunching_when_appPendingApproval', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.p', 'approval', 'PendingApp')])
    const nav = useShellNavigation()

    const ok = await openShellApp('com.test.p')

    expect(ok).toBe(true)
    expect(source.launch, '待审批应用启动必被后端拒绝，不能试').not.toHaveBeenCalled()
    expect(nav.current.value.id).toBe('app-detail')
    expect(nav.params.value.appId).toBe('com.test.p')
  })

  it('should_launchAndEnterSurface_when_appIsStartable', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])
    const nav = useShellNavigation()

    const ok = await openShellApp('com.test.a')

    expect(ok).toBe(true)
    expect(source.launch).toHaveBeenCalledWith('com.test.a')
    expect(nav.current.value.id).toBe('app-run')
  })

  it('should_stayPut_when_launchFails', async () => {
    shell.upsertApps(SOURCE_ID, [app('com.test.a', 'stopped', 'AppA')])
    source.launch.mockRejectedValueOnce(new Error('boom'))
    const nav = useShellNavigation()

    const ok = await openShellApp('com.test.a')

    expect(ok).toBe(false)
    expect(nav.current.value.id, '启动失败不进运行面（错误留在列表上可见）').toBe('home')
  })
})
