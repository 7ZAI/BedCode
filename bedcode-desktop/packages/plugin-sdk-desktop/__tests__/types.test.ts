/**
 * 类型契约测试（expectTypeOf，编译期断言）
 *
 * types.ts 与宿主 bedcode-desktop/src/plugin/types.ts 为双写副本，
 * 此处锁定判别联合与 manifest 结构，防止任一侧漂移。
 */
import { describe, it, expectTypeOf } from 'vitest'
import type {
  PluginState,
  PluginManifest,
  PluginType,
  PluginContributes,
  ConfigProperty,
} from '../src/types'

describe('PluginState 判别联合形状', () => {
  it('四态精确匹配（Error 携带 error 字符串）', () => {
    expectTypeOf<PluginState>().toEqualTypeOf<
      | { state: 'Loaded' }
      | { state: 'Activated' }
      | { state: 'Error'; error: string }
      | { state: 'Deactivated' }
    >()
  })
})

describe('PluginManifest 结构', () => {
  it('必需字段齐全且类型正确', () => {
    expectTypeOf<PluginManifest>().toMatchTypeOf<{
      id: string
      name: string
      version: string
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
})

describe('ConfigProperty 结构', () => {
  it('type 仅允许 string/number/boolean', () => {
    expectTypeOf<ConfigProperty>().toMatchTypeOf<{
      type: 'string' | 'number' | 'boolean'
      title: string
    }>()
  })
})
