/**
 * 任务域「壳内可达」注册契约测试
 * （票 2026-10-09 阶段 B 前置：任务页原本只挂在旧宿主工具箱，旧宿主退役后必须有壳内入口）
 *
 * 被测：`src/task/activate.ts` 的 `activateTaskDomain` / `deactivateTaskDomain`。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-T1 | registerRoute | 任务页注册为插件动态路由（id `tasks`，host 页头模式自带返回） | 入参形状 + header true |
 * | C-T2 | registerCapsuleItem | 壳内应用菜单项（id `task-page`，文案走 `context.i18n`） | 入参形状 |
 * | C-T3 | onSelect 副作用 | 胶囊项点击 → `ui.openPage('tasks')` | 调用一次且路由 id 正确 |
 * | C-T4 | 停用回收 | 路由 / 胶囊 / 工具箱 / 工具栏 disposable 各回收一次 | dispose 各 1 次 |
 * | C-T5 | supported-agents 门控 | 受保护端点只在**已认证（paired）**时请求（401 回归） | 未配对 0 次 / 配对 1 次 / 认证后补拉 1 次 |
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { nextTick, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import { activateTaskDomain, deactivateTaskDomain } from '../activate'
import AutoTaskToolboxView from '../components/AutoTaskToolboxView.vue'
import {
  installMockMobileApi,
  uninstallMockMobileApi,
} from '../../terminal/__tests__/testKit'

/** 注册项替身：dispose 计数，便于断言回收 */
function disposable(counters: Record<string, number>, key: string) {
  return {
    dispose() {
      counters[key] += 1
    },
  }
}

function makeFakeContext() {
  const counters: Record<string, number> = { route: 0, capsule: 0 }
  const calls = { openPage: [] as string[] }
  const ctx = {
    i18n: {
      registerMessages: vi.fn(),
      t: (key: string) => key,
    },
    ui: {
      registerRoute: vi.fn(() => disposable(counters, 'route')),
      registerCapsuleItem: vi.fn(() => disposable(counters, 'capsule')),
      openPage: vi.fn((id: string) => {
        calls.openPage.push(id)
      }),
    },
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
  } as unknown as PluginContext
  return { ctx, counters, calls }
}

let fake: ReturnType<typeof makeFakeContext>

/**
 * 预设任务共享模块替身
 *
 * 任务面板（常驻 document.body，activate 期挂载）在 setup 里经 SDK `getPresetTasks()`
 * 取宿主预设任务机制；单测没有宿主共享运行时，这里按面板实际消费的形状给出替身。
 */
function installPresetTasksStub(): void {
  const shared = ((window as any).__BEDCODE_SHARED__ ??= {})
  shared.presetTasks = {
    usePresetTasks: () => ({
      tasks: ref([]),
      markEnqueued: vi.fn(),
      markCompletedByTaskId: vi.fn(),
      markInterruptedByTaskId: vi.fn(),
      revertToUnusedByTaskId: vi.fn(),
      reconcileWithQueue: vi.fn(),
      canEnqueue: () => true,
    }),
  }
}

beforeEach(() => {
  installMockMobileApi({ isConnected: false })
  installPresetTasksStub()
  fake = makeFakeContext()
})

afterEach(() => {
  deactivateTaskDomain()
  uninstallMockMobileApi()
})

describe('C-T1/C-T2 壳内任务页注册', () => {
  it('should_registerRouteAndCapsuleItem_when_taskDomainActivated', async () => {
    await activateTaskDomain(fake.ctx)

    expect(fake.ctx.ui.registerRoute).toHaveBeenCalledTimes(1)
    expect(fake.ctx.ui.registerRoute).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'tasks', header: true, component: AutoTaskToolboxView }),
    )

    expect(fake.ctx.ui.registerCapsuleItem).toHaveBeenCalledTimes(1)
    expect(fake.ctx.ui.registerCapsuleItem).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'task-page', label: 'capsuleTitle' }),
    )
  })
})

describe('C-T3 胶囊项副作用', () => {
  it('should_openTaskPage_when_capsuleSelected', async () => {
    await activateTaskDomain(fake.ctx)

    const item = (fake.ctx.ui.registerCapsuleItem as unknown as { mock: { calls: any[][] } }).mock
      .calls[0][0] as { onSelect: () => void }
    item.onSelect()

    expect(fake.calls.openPage).toEqual(['tasks'])
  })
})

describe('C-T4 停用回收', () => {
  it('should_disposeEachRegistrationOnce_when_taskDomainDeactivated', async () => {
    await activateTaskDomain(fake.ctx)
    deactivateTaskDomain()

    expect(fake.counters.route).toBe(1)
    expect(fake.counters.capsule).toBe(1)
  })
})

describe('C-T5 适配 agent 白名单门控（受保护端点）', () => {
  /** 计数 supported-agents 请求（其余路径不计数） */
  function makeCounter() {
    const state = { count: 0 }
    return {
      state,
      httpRequest: async (path: string) => {
        if (path.includes('/supported-agents')) state.count += 1
        return { code: 0, message: 'ok', data: { agents: ['claude-code'] } }
      },
    }
  }

  it('should_skipSupportedAgents_when_activatedBeforeAuthentication', async () => {
    // 回归：旧实现看 isConnected（传输层刚连上即 true）就在 activate 期发请求，
    // 桌面端因无 JWT 回 401，且 isConnected 不再翻转 → 白名单永远为空
    const counter = makeCounter()
    installMockMobileApi({ isConnected: true, isPaired: false, httpRequest: counter.httpRequest })

    await activateTaskDomain(fake.ctx)

    expect(counter.state.count).toBe(0)
  })

  it('should_fetchSupportedAgents_when_activatedAlreadyAuthenticated', async () => {
    const counter = makeCounter()
    installMockMobileApi({ isConnected: true, isPaired: true, httpRequest: counter.httpRequest })

    await activateTaskDomain(fake.ctx)

    expect(counter.state.count).toBe(1)
  })

  it('should_fetchAgain_when_authenticationCompletesAfterActivation', async () => {
    const counter = makeCounter()
    const api = installMockMobileApi({
      isConnected: true,
      isPaired: false,
      httpRequest: counter.httpRequest,
    })
    await activateTaskDomain(fake.ctx)
    expect(counter.state.count).toBe(0)

    // 认证完成（connected → paired）→ watch 补拉
    api.isPaired.value = true
    await nextTick()

    expect(counter.state.count).toBe(1)
  })
})
