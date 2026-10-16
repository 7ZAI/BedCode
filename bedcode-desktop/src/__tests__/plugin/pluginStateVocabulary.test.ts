/**
 * 插件状态词汇锁（2026-10-14 桌面端 camelCase 修复配套）
 *
 * 背景：宿主插件状态的真源是共享内核 crate 的
 * `packages/bedcode-wasm-core/src/manifest.rs` —— `#[serde(tag = "state",
 * rename_all = "camelCase")]`，线上 JSON 是 camelCase（`{state:'activated'}`）。
 * 桌面前端曾按 PascalCase（`'Activated'`）声明与分支，接口能编译、测试用同款
 * PascalCase 夹具自洽，于是整条状态判定链在真机上成为死代码：启停开关恒显示
 * 「未启用」（`isRunning` 永不命中）、已启用/未启用分区恒错位、贡献面永不生效。
 *
 * 本锁把「Rust 真源 → 两侧 TS 双写副本 → 生产消费面 → 判定行为」四层钉在一起：
 * - L1 Rust 真源的 serde 形状与变体集合（解析 manifest.rs 原文，不手抄）
 * - L2 宿主 `src/plugin/types.ts` 与 SDK `packages/plugin-sdk-desktop/src/types.ts`
 *   的 state 字面量集合与真源逐字相等（任一侧漂移或写回 PascalCase 即红）
 * - L3 生产消费面不再残留 PascalCase 旧词汇（防回接）
 * - L4 状态判定函数按线上值给出正例/反例（含「只认 camelCase、不兼容旧值」）
 * - L5 registry 贡献面生效门按 activated / degraded 放行，其余拒绝
 *
 * 注意路径基准：vitest cwd = bedcode-desktop，仓库根共享 crate 是 `../packages/...`。
 */

import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import {
  getStateKey,
  isActivated,
  isDegraded,
  isErrorState,
  isNeedsApproval,
  isRunning,
} from '@/plugin/contributionKinds'
import { isContributionActiveState } from '@/plugin/registry'
import type { PluginState } from '@/plugin/types'

const RUST_SOURCE = '../packages/bedcode-wasm-core/src/manifest.rs'
const HOST_TYPES = 'src/plugin/types.ts'
const SDK_TYPES = 'packages/plugin-sdk-desktop/src/types.ts'

/** 从 Rust 真源解析 `PluginState` 的线上值集合（变体名 → serde camelCase） */
function rustStateWireValues(): string[] {
  const source = readFileSync(RUST_SOURCE, 'utf-8')
  const declStart = source.indexOf('pub enum PluginState {')
  expect(declStart, `找不到 ${RUST_SOURCE} 的 PluginState 定义`).toBeGreaterThan(-1)

  // serde 属性必须紧邻枚举定义上方（扫前一段查找，重命名规则变化即红）
  const attrBlock = source.slice(Math.max(0, declStart - 400), declStart)
  expect(
    attrBlock,
    'PluginState 的 serde 形状变了：本锁按 tag="state" + rename_all="camelCase" 钉 TS 词汇，' +
      '改形状必须同步改动两处 TS 双写副本与本锁',
  ).toContain('#[serde(tag = "state", rename_all = "camelCase")]')

  // 花括号配平扫描枚举体（结构体变体自带花括号，不能按首个 `}` 截断）
  let depth = 0
  let bodyEnd = -1
  for (let i = source.indexOf('{', declStart); i < source.length; i++) {
    const ch = source[i]
    if (ch === '{') depth++
    else if (ch === '}') {
      depth--
      if (depth === 0) {
        bodyEnd = i
        break
      }
    }
  }
  expect(bodyEnd, 'PluginState 枚举体未闭合，解析失败').toBeGreaterThan(-1)

  // 4 空格缩进 + 大写开头 = 顶层变体（结构体变体字段缩进 8 空格，doc 注释不以大写标识符开头）
  const variants = source
    .slice(source.indexOf('{', declStart), bodyEnd)
    .split('\n')
    .map((line) => line.match(/^\s{4}([A-Z][A-Za-z0-9]*)\s*(?:\{|,)/)?.[1])
    .filter((name): name is string => name !== undefined)

  // 防「解析空转 = 全绿」：真源当前有 7 个变体，少一个即解析失效
  expect(variants.length, 'PluginState 变体解析结果少于 7 个，锁失效').toBeGreaterThanOrEqual(7)

  return variants.map((name) => name[0].toLowerCase() + name.slice(1)).sort()
}

/** 从 TS 双写副本解析 `PluginState` 联合的 state 字面量集合 */
function tsStateWireValues(file: string): string[] {
  const source = readFileSync(file, 'utf-8')
  const declStart = source.indexOf('export type PluginState =')
  expect(declStart, `找不到 ${file} 的 PluginState 定义`).toBeGreaterThan(-1)
  const block = source.slice(declStart).split('\n\n')[0]
  const values = [...block.matchAll(/state:\s*'([^']+)'/g)].map((m) => m[1])
  expect(values.length, `${file} 的 PluginState 联合解析为空，锁失效`).toBeGreaterThanOrEqual(7)
  return [...new Set(values)].sort()
}

/** 生产消费面（改这些文件时状态判定必须用线上值） */
const STATE_CONSUMERS = [
  'src/plugin/contributionKinds.ts',
  'src/plugin/loader.ts',
  'src/plugin/registry.ts',
  'src/plugin/commands.ts',
  'src/views/PluginsView.vue',
  'src/views/PluginDetailView.vue',
  'src/views/PluginConfigView.vue',
  'src/views/PluginWindowHostView.vue',
  'src/components/PluginApprovalDialog.vue',
]

/** 旧 PascalCase 词表（真源已是 camelCase，出现即回接） */
const RETIRED_PASCAL_STATES = [
  'Loaded',
  'Activating',
  'Activated',
  'Degraded',
  'NeedsApproval',
  'Error',
  'Deactivated',
]

/** 每个线上状态在各判定函数下的契约（正例与反例同表对照） */
const STATE_CONTRACTS: Array<{
  state: PluginState
  stateKey: string
  running: boolean
  activated: boolean
  degraded: boolean
  needsApproval: boolean
  error: boolean
  contributionActive: boolean
}> = [
  {
    state: { state: 'loaded' },
    stateKey: 'desktop.plugin.loaded',
    running: false,
    activated: false,
    degraded: false,
    needsApproval: false,
    error: false,
    contributionActive: false,
  },
  {
    state: { state: 'activating' },
    stateKey: 'desktop.plugin.activating',
    running: false,
    activated: false,
    degraded: false,
    needsApproval: false,
    error: false,
    contributionActive: false,
  },
  {
    state: { state: 'activated' },
    stateKey: 'desktop.plugin.activated',
    running: true,
    activated: true,
    degraded: false,
    needsApproval: false,
    error: false,
    contributionActive: true,
  },
  {
    state: { state: 'degraded', error: 'on_startup failed' },
    stateKey: 'desktop.plugin.degraded',
    running: true,
    activated: false,
    degraded: true,
    needsApproval: false,
    error: false,
    contributionActive: true,
  },
  {
    state: { state: 'needsApproval' },
    stateKey: 'desktop.plugin.needsApproval',
    running: false,
    activated: false,
    degraded: false,
    needsApproval: true,
    error: false,
    contributionActive: false,
  },
  {
    state: { state: 'error', error: 'boom' },
    stateKey: 'desktop.plugin.error',
    running: false,
    activated: false,
    degraded: false,
    needsApproval: false,
    error: true,
    contributionActive: false,
  },
  {
    state: { state: 'deactivated' },
    stateKey: 'desktop.plugin.deactivated',
    running: false,
    activated: false,
    degraded: false,
    needsApproval: false,
    error: false,
    contributionActive: false,
  },
]

describe('插件状态词汇锁（真源 = wasm-core manifest.rs camelCase）', () => {
  it('L1 真源 serde 形状与变体：tag=state + rename_all=camelCase，7 个变体可解析', () => {
    const values = rustStateWireValues()
    expect(values).toEqual([
      'activated',
      'activating',
      'deactivated',
      'degraded',
      'error',
      'loaded',
      'needsApproval',
    ])
  })

  it('L2 宿主与 SDK 的 TS 双写副本 state 字面量集合与真源逐字相等', () => {
    const expected = rustStateWireValues()
    expect(tsStateWireValues(HOST_TYPES), `${HOST_TYPES} 与真源漂移`).toEqual(expected)
    expect(tsStateWireValues(SDK_TYPES), `${SDK_TYPES} 与真源漂移`).toEqual(expected)
  })

  it('L3 生产消费面不再残留 PascalCase 旧词表（防回接）', () => {
    for (const file of STATE_CONSUMERS) {
      const source = readFileSync(file, 'utf-8')
      for (const retired of RETIRED_PASCAL_STATES) {
        for (const quote of ["'", '"']) {
          expect(
            source.includes(`${quote}${retired}${quote}`),
            `${file} 仍出现旧状态字面量 ${quote}${retired}${quote}——状态判定必须用 camelCase 线上值`,
          ).toBe(false)
        }
      }
    }
  })

  it('L4 判定函数逐态给正例/反例，且不兼容旧 PascalCase 值', () => {
    for (const contract of STATE_CONTRACTS) {
      const { state } = contract
      expect(isRunning(state), `isRunning(${state.state})`).toBe(contract.running)
      expect(isActivated(state), `isActivated(${state.state})`).toBe(contract.activated)
      expect(isDegraded(state), `isDegraded(${state.state})`).toBe(contract.degraded)
      expect(isNeedsApproval(state), `isNeedsApproval(${state.state})`).toBe(
        contract.needsApproval,
      )
      expect(isErrorState(state), `isErrorState(${state.state})`).toBe(contract.error)
      expect(getStateKey(state), `getStateKey(${state.state})`).toBe(contract.stateKey)
      expect(
        isContributionActiveState(state),
        `isContributionActiveState(${state.state})`,
      ).toBe(contract.contributionActive)
    }

    // 旧词表是「已退役的线上值」，不得为了兼容而双认：任一分支写回 PascalCase 即红
    const retiredWireShapes = [
      { state: 'Activated' },
      { state: 'Degraded', error: 'x' },
      { state: 'Error', error: 'x' },
      { state: 'Loaded' },
      { state: 'Activating' },
      { state: 'NeedsApproval' },
      { state: 'Deactivated' },
    ]
    for (const retired of retiredWireShapes) {
      const asState = retired as unknown as PluginState
      expect(isRunning(asState), `旧值 ${retired.state} 不得被判为运行中`).toBe(false)
      expect(isActivated(asState), `旧值 ${retired.state} 不得被判为已激活`).toBe(false)
      expect(isDegraded(asState), `旧值 ${retired.state} 不得被判为降级`).toBe(false)
      expect(isNeedsApproval(asState), `旧值 ${retired.state} 不得被判为待授权`).toBe(false)
      expect(isErrorState(asState), `旧值 ${retired.state} 不得被判为错误态`).toBe(false)
      expect(isContributionActiveState(asState), `旧值 ${retired.state} 不得生效贡献面`).toBe(
        false,
      )
    }
  })
})
