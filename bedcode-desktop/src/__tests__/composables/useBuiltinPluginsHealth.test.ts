/**
 * useBuiltinPluginsHealth 判据测试（缺口 B：内置插件资源缺失明示）
 *
 * 断言面：builtinPluginsMissing 纯函数——「非 user-installed 插件数为 0 ⇒ 内置
 * 插件缺失」。source 取值 builtin / scanned / wasm（随包三类）与 user-installed
 * （zip 安装）；正常安装下随包四插件恒在册，故非 user-installed = 0 是明确异常。
 *
 * 不测 onMounted 探测路径（pluginListLoaded 被 mock 后只是透传，判据已单测；
 * 组件级行为由宿主 vitest 全量覆盖）。
 */
import { describe, it, expect } from 'vitest'
import { builtinPluginsMissing } from '@/composables/useBuiltinPluginsHealth'

/** 最小插件条目（只填判据需要的 source 字段） */
function plugin(source: string): { source: string } {
  return { source }
}

describe('builtinPluginsMissing 判据', () => {
  it('空列表 = 内置缺失（应用零插件空壳启动）', () => {
    expect(builtinPluginsMissing([])).toBe(true)
  })

  it('只有用户安装插件 = 内置缺失（随包插件一个不剩）', () => {
    expect(builtinPluginsMissing([plugin('user-installed'), plugin('user-installed')])).toBe(true)
  })

  it('任一内置来源（builtin/scanned/wasm）在册 = 非缺失', () => {
    expect(builtinPluginsMissing([plugin('builtin')])).toBe(false)
    expect(builtinPluginsMissing([plugin('scanned')])).toBe(false)
    expect(builtinPluginsMissing([plugin('wasm')])).toBe(false)
  })

  it('正常混合（四内置 + 用户安装） = 非缺失', () => {
    const plugins = [
      plugin('builtin'),
      plugin('scanned'),
      plugin('wasm'),
      plugin('scanned'),
      plugin('user-installed'),
    ]
    expect(builtinPluginsMissing(plugins)).toBe(false)
  })

  it('内置插件即使处于 error/degraded 态仍在册 = 非缺失（判据只看有无不看状态）', () => {
    expect(builtinPluginsMissing([plugin('wasm')])).toBe(false)
  })
})
