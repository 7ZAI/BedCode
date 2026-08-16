/**
 * 类型契约测试（expectTypeOf，编译期断言）
 *
 * 移动端 types.ts 与桌面端存在差异（PluginType 含 wasm、PluginState
 * 无差异、contributes 含移动端专属扩展点），此处锁定移动端形状。
 */
import { describe, it, expectTypeOf } from 'vitest'
import type {
  PluginState,
  PluginManifest,
  PluginType,
  MobilePluginContributes,
  NavTabContribution,
} from '../src/types'

describe('PluginState 判别联合形状', () => {
  it('四态精确匹配（Error 携带 error 字符串）', () => {
    expectTypeOf<PluginState>().toEqualTypeOf<
      | { state: 'Loaded' }
      | { state: 'Activated' }
      | { state: 'Deactivated' }
      | { state: 'Error'; error: string }
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
      pluginType: PluginType
      permissions: string[]
      contributes: MobilePluginContributes
    }>()
  })

  it('pluginType 含 wasm（移动端特有）', () => {
    expectTypeOf<PluginType>().toEqualTypeOf<'rust' | 'rust-ts' | 'ts-only' | 'wasm'>()
  })
})

describe('移动端专属扩展点', () => {
  it('NavTabContribution 必需字段与 order 缺省', () => {
    expectTypeOf<NavTabContribution>().toMatchTypeOf<{
      id: string
      title: string
      icon: string
      component: string
    }>()
    expectTypeOf<NavTabContribution>().toHaveProperty('order').toEqualTypeOf<
      number | undefined
    >()
  })
})
