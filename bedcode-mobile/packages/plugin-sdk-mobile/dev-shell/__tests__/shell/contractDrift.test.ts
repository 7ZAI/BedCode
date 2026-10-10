/**
 * 宿主壳契约漂移锁（dev-shell ↔ 移动端宿主）
 * -----------------------------------------------------------------------------
 * dev-shell 的 mini 壳是宿主壳的**同构实现**（npm 自包含，不能 import app 源码），
 * 因此两者之间没有编译期约束——只能靠这道锁：宿主改了契约而 dev-shell 没跟上时，
 * 插件开发者会在预览环境里遇到与真机不同的行为，而这恰恰是 dev-shell 唯一不该发生的事。
 *
 * 比对对象：
 *   · ShellApp / ShellAppState / ShellPermissionGrant / ShellAppContributions 字段集
 *     ←→ bedcode-mobile/src/shell/types.ts
 *   · 屏幕 id 集合 ←→ 宿主的 ShellScreenId 联合类型
 *   · 贡献描述符字段集 ←→ SDK src/types.ts（dev-shell 与宿主共用同一真源，不允许各写一份）
 *
 * 宿主源码不在场时（npm 包内的 dev-shell）整组跳过：那时没有可比对象，跳过比
 * 「假装通过」诚实——真正的门禁在 monorepo 内跑。
 */
import { describe, it, expect } from 'vitest'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
/** dev-shell/__tests__/shell → 仓库内的移动端宿主：bedcode-mobile/src/shell */
const HOST_TYPES_PATH = resolve(here, '../../../../../src/shell/types.ts')
/** 宿主的屏幕栈定义在 composables 里，不在 types.ts */
const HOST_NAV_PATH = resolve(here, '../../../../../src/shell/composables/useShellNavigation.ts')
/** dev-shell/__tests__/shell → SDK 侧契约真源：packages/plugin-sdk-mobile/src/types.ts */
const SDK_TYPES_PATH = resolve(here, '../../../src/types.ts')

const hostTypesSource = existsSync(HOST_TYPES_PATH) ? readFileSync(HOST_TYPES_PATH, 'utf-8') : null
const hostNavSource = hostTypesSource && existsSync(HOST_NAV_PATH) ? readFileSync(HOST_NAV_PATH, 'utf-8') : null
const sdkTypesSource = readFileSync(SDK_TYPES_PATH, 'utf-8')

/**
 * 提取 interface 体的一级字段名
 *
 * 用源码文本而非 TS 反射：宿主 types.ts 只 import 了类型（会被擦除），但它不在
 * 本包的依赖图里，按路径 import 会把整个 app 拖进 vitest 依赖图——文本比对
 * 既轻又不会误伤构建。
 */
function fieldsOf(source: string, name: string): string[] {
  const start = source.indexOf(`interface ${name} `)
  if (start < 0) throw new Error(`interface ${name} not found`)
  const open = source.indexOf('{', start)
  let depth = 0
  let end = open
  for (let i = open; i < source.length; i += 1) {
    const ch = source[i]
    if (ch === '{') depth += 1
    else if (ch === '}') {
      depth -= 1
      if (depth === 0) {
        end = i
        break
      }
    }
  }
  const body = source.slice(open + 1, end)
  return [...body.matchAll(/^\s{2}([A-Za-z_$][\w$]*)(\?)?:/gm)].map((m) => m[1])
}

/**
 * 提取联合字面量类型里的成员（ShellAppState / ShellScreenId 这类）
 *
 * 成员间夹着文档注释且声明是多行的，因此取到空行为止的整段再匹配。
 */
function unionMembers(source: string, name: string): string[] {
  const start = source.indexOf(`type ${name} =`)
  if (start < 0) throw new Error(`type ${name} not found`)
  const blank = source.indexOf('\n\n', start)
  const declaration = source.slice(start, blank < 0 ? start + 4000 : blank)
  return [...declaration.matchAll(/'([^']+)'/g)].map((m) => m[1])
}

const describeIfHostPresent = hostTypesSource ? describe : describe.skip

describeIfHostPresent('ShellApp 契约与宿主一致', () => {
  it('should_matchHostFields_when_ShellAppCompared', () => {
    const host = fieldsOf(hostTypesSource!, 'ShellApp')
    const local = fieldsOf(
      readFileSync(resolve(here, '../../src/shell/types.ts'), 'utf-8'),
      'ShellApp',
    )

    // 宿主新增字段而 dev-shell 没跟 → 直接测红（反例方向）；本地多出的字段同样测红，
    // 因为多出来的字段在真机上根本不存在，壳会去读一个永远为 undefined 的值
    expect([...local].sort()).toEqual([...host].sort())
  })

  it('should_matchHostFields_when_PermissionGrantCompared', () => {
    const host = fieldsOf(hostTypesSource!, 'ShellPermissionGrant')
    const local = fieldsOf(
      readFileSync(resolve(here, '../../src/shell/types.ts'), 'utf-8'),
      'ShellPermissionGrant',
    )

    expect([...local].sort()).toEqual([...host].sort())
  })

  it('should_matchHostStates_when_AppStateCompared', () => {
    const host = unionMembers(hostTypesSource!, 'ShellAppState')
    const local = unionMembers(
      readFileSync(resolve(here, '../../src/shell/types.ts'), 'utf-8'),
      'ShellAppState',
    )

    // 少一个态：预览里某状态会落进 default 分支，与真机表现不一致
    expect([...local].sort()).toEqual([...host].sort())
  })

  it('should_matchHostSourceCapabilityNames_when_AppSourceCompared', () => {
    const host = fieldsOf(hostTypesSource!, 'ShellAppSource')
    const local = fieldsOf(
      readFileSync(resolve(here, '../../src/shell/types.ts'), 'utf-8'),
      'ShellAppSource',
    )

    // 数据源能力名是「壳按能力渲染 UI」的依据：本地多一个能力会让壳渲染出真机没有的入口
    expect([...local].sort()).toEqual([...host].sort())
  })
})

describeIfHostPresent('屏幕栈与宿主一致', () => {
  it('should_matchHostScreenIds_when_navigationCompared', () => {
    const host = unionMembers(hostNavSource!, 'ShellScreenId')
    const local = unionMembers(
      readFileSync(resolve(here, '../../src/shell/composables/useShellNavigation.ts'), 'utf-8'),
      'ShellScreenId',
    )

    // 屏幕集合不同 = 预览里能进的屏与真机不同（或反之）
    expect([...local].sort()).toEqual([...host].sort())
  })
})

describe('贡献描述符共用 SDK 真源', () => {
  const localTypes = readFileSync(resolve(here, '../../src/shell/types.ts'), 'utf-8')

  it.each([
    'ShellSurfaceContribution',
    'ShellSlotContribution',
    'ShellCapsuleItem',
    'ShellSettingsEntry',
  ])('should_reuseSdkDeclaration_when_%sIsLocal', (name) => {
    // dev-shell 不得重定义贡献描述符：重定义就等于第三处可能漂移的地方
    expect(localTypes).not.toContain(`interface ${name}`)
    expect(sdkTypesSource).toContain(`interface ${name}`)
  })
})