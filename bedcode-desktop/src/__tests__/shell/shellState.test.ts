import { describe, it, expect } from 'vitest'
import {
  APP_TONES,
  HIGH_RISK_PERMISSIONS,
  SYS_PAGES,
  WALLPAPER_DEFAULT,
  appCardKey,
  appDisplayState,
  approvalApps,
  cardId,
  cardKind,
  isHighRisk,
  isRunningState,
  mruActivate,
  mruRemove,
  notifCount,
  sysCardKey,
  toneOf,
} from '@/shell/state'
import type { PluginInfo } from '@/plugin/types'

/**
 * 壳纯逻辑助手测试（state.ts 无 Vue 依赖，全部确定性纯函数）
 *
 * 契约来源：.scratch/2026-10-08-desktop-shell-redesign/spec-c.md §1（卡片 MRU /
 * 关卡通配语义 / 待审批派生）与 §4（tone 机制哈希）。
 */

function fakeApp(id: string, state: PluginInfo['state']): PluginInfo {
  return {
    id,
    name: id,
    version: '1.0.0',
    description: '',
    author: '',
    main: '',
    pluginType: 'rust-ts',
    permissions: [],
    state,
    extensionPath: '',
    contributes: { commands: [], views: [], toolProviders: [], fileHandlers: [] },
    source: 'builtin',
    sizeBytes: 0,
  }
}

describe('卡片键', () => {
  it('app / sys 键构造与解析互逆', () => {
    expect(appCardKey('terminal-session')).toBe('app:terminal-session')
    expect(sysCardKey('settings')).toBe('sys:settings')
    expect(cardKind(appCardKey('x'))).toBe('app')
    expect(cardKind(sysCardKey('x'))).toBe('sys')
    expect(cardId(appCardKey('a:b:c'))).toBe('a:b:c')
    expect(cardId(sysCardKey('apps'))).toBe('apps')
  })
})

describe('卡片 MRU（spec-c §1：最新在前，重复激活只重排不重复产卡）', () => {
  it('新卡排最前', () => {
    expect(mruActivate([], 'app:a')).toEqual(['app:a'])
    expect(mruActivate(['app:a'], 'sys:s')).toEqual(['sys:s', 'app:a'])
  })

  it('重复激活只重排、不产重复卡', () => {
    expect(mruActivate(['app:a', 'app:b'], 'app:a')).toEqual(['app:a', 'app:b'])
    expect(mruActivate(['app:b', 'app:a'], 'app:a')).toEqual(['app:a', 'app:b'])
    expect(mruActivate(['app:a'], 'app:a')).toEqual(['app:a'])
  })

  it('移除：删除指定键，不影响其余顺序', () => {
    expect(mruRemove(['app:a', 'app:b', 'sys:s'], 'app:b')).toEqual(['app:a', 'sys:s'])
    expect(mruRemove(['app:a'], 'app:a')).toEqual([])
    expect(mruRemove([], 'app:a')).toEqual([])
  })
})

describe('展示态归并（PluginState → 壳五档）', () => {
  it('activated → running；degraded → degraded（实例在跑）', () => {
    expect(appDisplayState({ state: 'activated' })).toBe('running')
    expect(appDisplayState({ state: 'degraded', error: 'x' })).toBe('degraded')
    expect(isRunningState({ state: 'activated' })).toBe(true)
    expect(isRunningState({ state: 'degraded', error: 'x' })).toBe(true)
  })

  it('needsApproval → approval（进审批屏卡）；error → error', () => {
    expect(appDisplayState({ state: 'needsApproval' })).toBe('approval')
    expect(appDisplayState({ state: 'error', error: 'x' })).toBe('error')
    expect(isRunningState({ state: 'needsApproval' })).toBe(false)
    expect(isRunningState({ state: 'error', error: 'x' })).toBe(false)
  })

  it('未启动三态（loaded / activating / deactivated）与未知态一律归 dormant', () => {
    expect(appDisplayState({ state: 'loaded' })).toBe('dormant')
    expect(appDisplayState({ state: 'activating' })).toBe('dormant')
    expect(appDisplayState({ state: 'deactivated' })).toBe('dormant')
    expect(appDisplayState(undefined)).toBe('dormant')
  })
})

describe('待审批派生与计数（spec-c §7：动态派生，无独立生命周期）', () => {
  const apps = [
    fakeApp('a', { state: 'needsApproval' }),
    fakeApp('b', { state: 'activated' }),
    fakeApp('c', { state: 'needsApproval' }),
  ]

  it('只派生 needsApproval 应用，批准后（状态离开 approval）自然消失', () => {
    expect(approvalApps(apps).map((a) => a.id)).toEqual(['a', 'c'])
    const after = apps.map((a) => (a.id === 'a' ? fakeApp('a', { state: 'activated' }) : a))
    expect(approvalApps(after).map((a) => a.id)).toEqual(['c'])
  })

  it('notifCount = 通知列表 + 待审批数', () => {
    expect(notifCount([], approvalApps(apps))).toBe(2)
    expect(
      notifCount(
        [{ id: 'n1', kind: 'event', icon: 'x', source: 's', title: 't', mins: 0, sticky: false }],
        approvalApps(apps),
      ),
    ).toBe(3)
  })
})

describe('高危权限位（manifest 声明里真正危险的位）', () => {
  it('process/pty/terminal-input/database-main 命中，其余不命中', () => {
    expect(isHighRisk('pty:spawn')).toBe(true)
    expect(isHighRisk('process:run')).toBe(true)
    expect(isHighRisk('terminal:input')).toBe(true)
    expect(isHighRisk('database:main')).toBe(true)
    expect(isHighRisk('fs:read')).toBe(false)
    expect(isHighRisk('network:http')).toBe(false)
    expect(HIGH_RISK_PERMISSIONS.size).toBe(4)
  })
})

describe('应用图标色调（机制级哈希，spec-c §4：零产品映射）', () => {
  it('同一插件 id 色调确定（幂等）', () => {
    expect(toneOf('terminal-session')).toBe(toneOf('terminal-session'))
    expect(toneOf('ai-chatbox')).toBe(toneOf('ai-chatbox'))
  })

  it('返回值始终在合法档位内', () => {
    for (const id of [
      'terminal-session',
      'ai-chatbox',
      'file-transfer',
      'agent-hub',
      'mobile-bridge',
      'zzz',
    ]) {
      expect(APP_TONES).toContain(toneOf(id))
    }
  })

  it('不同 id 覆盖多个档位（哈希分布不塌缩到单档）', () => {
    const tones = new Set(
      [
        'terminal-session',
        'ai-chatbox',
        'file-transfer',
        'agent-hub',
        'mobile-bridge',
        'com.example.one',
        'com.example.two',
        'com.example.three',
        'com.example.four',
      ].map(toneOf),
    )
    expect(tones.size).toBeGreaterThanOrEqual(2)
  })
})

describe('系统页清单与壁纸常量', () => {
  it('系统页三页齐全：apps / perms / settings', () => {
    expect(SYS_PAGES.map((s) => s.id)).toEqual(['apps', 'perms', 'settings'])
  })
  it('壁纸清单含 none + 三张 token 派生渐变，默认 aurora', () => {
    expect(WALLPAPER_DEFAULT).toBe('aurora')
  })
})
