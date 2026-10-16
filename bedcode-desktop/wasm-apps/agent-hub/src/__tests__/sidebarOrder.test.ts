/**
 * 侧边栏槽位契约测试（2026-10-04 排序调整：Agent Hub 移到 Agent任务 之下）
 *
 * 被测契约（外部可见产物，不测内部实现）：
 * - O1 双源同序：`plugin.json` 的 `contributes.views[0].order` 与入口
 *   `AGENT_HUB_SIDEBAR_ORDER` 必须相等。两条排序源各自独立生效——
 *   前端 `registerSidebarPanel({ order })` 决定侧边栏菜单顺序，宿主 Rust
 *   `registry.register_views(&m.id, &m.contributes.views)` 按 manifest 登记
 *   同一份视图。任一处漂移即出现「菜单在一处、面板列表在另一处」的错位，
 *   且不会有任何运行期报错。
 * - O2 槽位落点：215 严格位于 terminal-session 的 Agent任务槽位（210）与
 *   file-transfer 槽位（220）之间 —— 这正是「Agent Hub 排在 Agent任务
 *   下面」这条产品决策的可执行形式（宿主 useSidebarMenu 按 order 升序排布）。
 *
 * 反例（O1）：manifest 写回 240 而入口仍是 215 → O1 测红。
 * 反例（O2）：入口改成 240（旧值，排在 file-transfer 之后）→ O2 测红。
 */

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { AGENT_HUB_SIDEBAR_ORDER } from '../index'

// vitest 以宿主工作区根为 cwd 启动（bedcode-desktop/），据此定位插件工程根
const PLUGIN_ROOT = resolve(process.cwd(), 'wasm-apps/agent-hub')
const manifest = JSON.parse(readFileSync(resolve(PLUGIN_ROOT, 'plugin.json'), 'utf-8'))

describe('侧边栏槽位排序契约', () => {
  it('manifest 视图 order 与入口注册 order 同源', () => {
    const view = manifest.contributes.views.find(
      (v: { id: string }) => v.id === 'agent-hub.sidebar',
    )
    expect(view).toBeDefined()
    expect(view.order).toBe(AGENT_HUB_SIDEBAR_ORDER)
  })

  it('槽位紧随 Agent任务(210) 之后、file-transfer(220) 之前', () => {
    // 相邻槽位是 terminal-session 的 Agent任务 与 file-transfer 的侧边栏目录
    expect(AGENT_HUB_SIDEBAR_ORDER).toBe(215)
    expect(AGENT_HUB_SIDEBAR_ORDER).toBeGreaterThan(210)
    expect(AGENT_HUB_SIDEBAR_ORDER).toBeLessThan(220)
  })
})