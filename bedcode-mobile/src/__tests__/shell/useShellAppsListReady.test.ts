/**
 * `useShellApps.listReady` 行为契约测试
 * （2026-10-16：区分「清单未落地」与「应用不存在」，修「插件设置页返回闪应用不存在」）
 *
 * 被测：`src/shell/composables/useShellApps.ts` 的 `listReady` 与 `refresh()`。
 *
 * | 契约 | 来源 | 规则 | 预期 |
 * |---|---|---|---|
 * | C-L1 | 初始态 | 还没拉过清单 | listReady=false（UI 出加载态，不出空态） |
 * | C-L2 | refresh 成功 | 清单真的拉到应用 | listReady=true，且清单非空 |
 * | C-L3 | refresh 空清单 | 数据源返回 0 项也算「拉完了」 | listReady=true |
 * | C-L4 | refresh 失败 | 某个数据源 list() 抛错 | listReady 仍=true（不能永远转圈） |
 *
 * 为什么必须独立测这一层：ShellAppSurface / ShellAppRunScreen 的组件测试把
 * `useShellApps` 整个 mock 掉了，只验到「组件认得这个开关」，验不到开关本身会不会翻。
 * 少了本文件，把 refresh 里的 `listReady.value = true` 删掉不会有任何测试变红。
 *
 * 隔离手段：`listReady` 与注册表都是模块级单例，故每个用例 `vi.resetModules()` 后
 * **一起**动态 import `registry` 与 `useShellApps`——两者必须来自同一份新模块图，
 * 否则注册表是旧实例、refresh 找不到任何数据源，用例会在「0 源」分支上恒真通过。
 * C-L2 顺带断言清单非空，把「源真的被读到了」钉住。
 */
import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { ShellAppSource } from '@/shell/types'

const SOURCE_ID = 'list-ready-source'

/** useShellApps 的返回形状（本文件只用到这三项） */
interface FreshCtx {
  listReady: { value: boolean }
  refresh: () => Promise<void>
  apps: { value: unknown[] }
}

/** 取一份全新的模块图（注册表 + composable 同源），并注册一个可控数据源 */
async function fresh(listImpl: () => Promise<any[]>): Promise<FreshCtx> {
  vi.resetModules()
  const { getShellRegistry } = await import('@/shell/registry')
  const { useShellApps } = await import('@/shell/composables/useShellApps')
  const shell = getShellRegistry()
  const source: ShellAppSource = {
    id: SOURCE_ID,
    list: listImpl,
    launch: async () => {},
    stop: async () => {},
  }
  shell.registerSource(source)
  return useShellApps() as unknown as FreshCtx
}

const ONE_APP = { id: 'a', name: 'A', version: '1', state: 'stopped', permissions: [], contributions: {} }

describe('C-L1 初始态', () => {
  it('should_beNotReady_when_refreshNeverRan', async () => {
    const { listReady } = await fresh(async () => [])

    // 反例防护：这里若已是 true，UI 就会在清单到达前先闪「应用不存在」
    expect(listReady.value).toBe(false)
  })
})

describe('C-L2/C-L3 refresh 成功', () => {
  it('should_becomeReadyAndPopulateList_when_listResolvedWithApps', async () => {
    const { listReady, apps, refresh } = await fresh(async () => [ONE_APP])
    await refresh()

    expect(listReady.value).toBe(true)
    // 钉住「数据源确实被读到」：否则本用例可能在 0 源分支上恒真通过
    expect(apps.value).toHaveLength(1)
  })

  it('should_becomeReady_when_listResolvedEmpty', async () => {
    // 空清单也是「拉完了」——真机上零应用时不得停在加载态
    const { listReady, apps, refresh } = await fresh(async () => [])
    await refresh()

    expect(listReady.value).toBe(true)
    expect(apps.value).toHaveLength(0)
  })
})

describe('C-L4 refresh 失败', () => {
  it('should_becomeReadyAnyway_when_sourceListThrows', async () => {
    const { listReady, refresh } = await fresh(async () => {
      throw new Error('source down')
    })
    await refresh()

    // 拉取失败必须让位给空态，否则用户永远只看到转圈、看不到任何原因
    expect(listReady.value).toBe(true)
  })
})