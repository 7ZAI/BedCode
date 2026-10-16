/**
 * 类型契约测试（expectTypeOf，编译期断言）
 *
 * types.ts 与宿主 bedcode-desktop/src/plugin/types.ts 为双写副本，
 * 此处锁定判别联合与 manifest 结构，防止任一侧漂移。
 *
 * PluginState 的取值是**线上 camelCase**（共享内核
 * packages/bedcode-wasm-core/src/manifest.rs 的 serde 形状）；与 Rust 真源的
 * 逐字对照由宿主 vitest `src/__tests__/plugin/pluginStateVocabulary.test.ts`
 * 运行时钉住（本文件只在 `vitest --typecheck` 下生效）。
 */
import { describe, it, expectTypeOf } from 'vitest'
import type {
  PluginState,
  PluginManifest,
  PluginType,
  PluginPlatform,
  PluginContributes,
  ConfigProperty,
} from '../src/types'

describe('PluginState 判别联合形状', () => {
  it('七态精确匹配 camelCase 线上值（error/degraded 携带 error 字符串）', () => {
    expectTypeOf<PluginState>().toEqualTypeOf<
      | { state: 'loaded' }
      | { state: 'activating' }
      | { state: 'activated' }
      | { state: 'degraded'; error: string }
      | { state: 'needsApproval' }
      | { state: 'error'; error: string }
      | { state: 'deactivated' }
    >()
  })
})

describe('PluginManifest 结构', () => {
  it('必需字段齐全且类型正确', () => {
    expectTypeOf<PluginManifest>().toMatchTypeOf<{
      id: string
      name: string
      version: string
      platform: PluginPlatform
      description: string
      author: string
      main: string
      sandbox: 'inline' | 'isolated'
      pluginType: PluginType
      permissions: string[]
      contributes: PluginContributes
    }>()
  })

  it('pluginType 仅允许三种取值', () => {
    expectTypeOf<PluginType>().toEqualTypeOf<'rust' | 'rust-ts' | 'ts-only'>()
  })

  it('platform 仅允许三个 wire 值且必填', () => {
    expectTypeOf<PluginPlatform>().toEqualTypeOf<'desktop' | 'mobile' | 'both'>()
    expectTypeOf<PluginManifest>().toHaveProperty('platform').toEqualTypeOf<PluginPlatform>()
  })
})

describe('ConfigProperty 结构', () => {
  it('type 仅允许 string/number/boolean', () => {
    expectTypeOf<ConfigProperty>().toMatchTypeOf<{
      type: 'string' | 'number' | 'boolean'
      title: string
    }>()
  })
})
