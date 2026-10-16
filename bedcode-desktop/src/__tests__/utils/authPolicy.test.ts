/**
 * 应用授权读模型展示助手（utils/authPolicy）单元测试
 *
 * 契约来源：spec §9.1（按风险排序、始终允许置顶）+ 宿主 `AuthStrategy::parse`
 * 的 fail-safe（未知档位 = 默认档）。被测对象是纯函数，但断言的是**界面决策**：
 * 应用顺序、档位归一、记录计数——前端不依据这些字段做任何放行决策。
 */
import { describe, it, expect } from 'vitest'
import {
  autoAllowedRecords,
  deniedRecords,
  effectKeySuffix,
  firstPartyLabel,
  normalizeStrategy,
  opsKeySuffix,
  recordCount,
  recordsOf,
  riskRank,
  sortAppsByRisk,
  sourceKeySuffix,
  strategyOf,
  userGrantedRecords,
  type AuthRecord,
  type PluginAuthOverview,
  type ResourceStrategy,
} from '@/utils/authPolicy'

/** 最小读模型构造器：只显式写出用例关心的字段，其余给同构默认值 */
function app(overrides: Partial<PluginAuthOverview> & { pluginId: string }): PluginAuthOverview {
  return {
    name: overrides.pluginId,
    strategies: [
      { resource: 'fs', strategy: 'default' },
      { resource: 'network', strategy: 'default' },
    ],
    records: [],
    firstPartyDirs: [],
    ...overrides,
  }
}

function record(overrides: Partial<AuthRecord> & { id: number; resource: string }): AuthRecord {
  return {
    pluginId: 'com.bedcode.test',
    target: '/target',
    effect: 'allow',
    ops: [],
    prefixMatch: false,
    source: 'user',
    createdAt: 0,
    ...overrides,
  }
}

/** 读模型里的策略条目（构造用例数据用） */
function strategy(resource: string, value: string): ResourceStrategy {
  return { resource, strategy: value }
}

describe('normalizeStrategy', () => {
  it('识别三个合法档位，未知值与缺项一律回落默认档', () => {
    expect(normalizeStrategy('always_ask')).toBe('always_ask')
    expect(normalizeStrategy('always_allow')).toBe('always_allow')
    expect(normalizeStrategy('default')).toBe('default')
    // fail-safe 方向：不认识 ≠ 更宽松（宿主同向，见 AuthStrategy::parse）
    expect(normalizeStrategy('bypass')).toBe('default')
    expect(normalizeStrategy('ALWAYS_ALLOW')).toBe('default')
    expect(normalizeStrategy(undefined)).toBe('default')
  })
})

describe('strategyOf', () => {
  it('取指定资源的档位；读模型缺该资源时按默认档', () => {
    const target = app({
      pluginId: 'com.bedcode.test',
      strategies: [strategy('fs', 'always_ask'), strategy('network', 'always_allow')],
    })
    expect(strategyOf(target, 'fs')).toBe('always_ask')
    expect(strategyOf(target, 'network')).toBe('always_allow')

    const partial = app({ pluginId: 'com.bedcode.partial', strategies: [strategy('fs', 'always_ask')] })
    expect(strategyOf(partial, 'network')).toBe('default')
  })
})

describe('recordCount', () => {
  it('只统计指定资源的记录（allow 与 deny 都计入）', () => {
    const target = app({
      pluginId: 'com.bedcode.test',
      records: [
        record({ id: 1, resource: 'fs', target: '/a', effect: 'allow' }),
        record({ id: 2, resource: 'fs', target: '/b', effect: 'deny' }),
        record({ id: 3, resource: 'network', target: 'https://a.com:443' }),
      ],
    })
    expect(recordCount(target, 'fs')).toBe(2)
    expect(recordCount(target, 'network')).toBe(1)
    expect(recordCount(target, 'pty')).toBe(0)
  })
})

describe('riskRank', () => {
  it('取两类资源里风险最高的档位：任一资源始终允许即置顶', () => {
    const onlyNetworkLoose = app({
      pluginId: 'com.bedcode.a',
      strategies: [strategy('fs', 'always_ask'), strategy('network', 'always_allow')],
    })
    expect(riskRank(onlyNetworkLoose)).toBe(0)

    const strict = app({
      pluginId: 'com.bedcode.b',
      strategies: [strategy('fs', 'always_ask'), strategy('network', 'always_ask')],
    })
    expect(riskRank(strict)).toBe(2)
  })

  it('读模型没有任何策略条目时按默认档权重（缺数据 ≠ 更安全）', () => {
    expect(riskRank(app({ pluginId: 'com.bedcode.empty', strategies: [] }))).toBe(
      riskRank(app({ pluginId: 'com.bedcode.default' })),
    )
  })
})

describe('recordsOf / sourceKeySuffix / effectKeySuffix / opsKeySuffix', () => {
  it('recordsOf 只取指定资源的记录', () => {
    const target = app({
      pluginId: 'com.bedcode.test',
      records: [
        record({ id: 1, resource: 'fs', target: '/a' }),
        record({ id: 2, resource: 'network', target: 'https://api.x.com:443' }),
        record({ id: 3, resource: 'fs', target: '/b' }),
      ],
    })
    expect(recordsOf(target, 'fs').map((r) => r.target)).toEqual(['/a', '/b'])
    expect(recordsOf(target, 'network').map((r) => r.target)).toEqual(['https://api.x.com:443'])
    expect(recordsOf(target, 'pty')).toEqual([])
  })

  it('sourceKeySuffix 只认已知来源，未知来源返回 null（界面回落原文而不是错标）', () => {
    expect(sourceKeySuffix('user')).toBe('user')
    expect(sourceKeySuffix('always_allow')).toBe('always_allow')
    expect(sourceKeySuffix('legacy')).toBe('legacy')
    expect(sourceKeySuffix('user_deny')).toBe('user_deny')
    expect(sourceKeySuffix('always_allow_v2')).toBeNull()
    expect(sourceKeySuffix('')).toBeNull()
  })

  it('effectKeySuffix 只认 allow / deny', () => {
    expect(effectKeySuffix('allow')).toBe('allow')
    expect(effectKeySuffix('deny')).toBe('deny')
    expect(effectKeySuffix('auto')).toBeNull()
  })

  it('opsKeySuffix 按操作集归一为 read / write / read_write，空集与未知操作返回 null', () => {
    expect(opsKeySuffix(['read'])).toBe('read')
    expect(opsKeySuffix(['write'])).toBe('write')
    expect(opsKeySuffix(['read', 'write'])).toBe('read_write')
    expect(opsKeySuffix(['write', 'read'])).toBe('read_write')
    expect(opsKeySuffix([])).toBeNull()
    expect(opsKeySuffix(['execute'])).toBeNull()
  })
})

describe('sortAppsByRisk', () => {
  it('始终允许置顶 → 默认 → 总是询问排末（spec §9.1）', () => {
    const zetaStrict = app({
      pluginId: 'com.bedcode.zeta',
      name: 'Zeta',
      strategies: [strategy('fs', 'always_ask'), strategy('network', 'always_ask')],
    })
    const alphaDefault = app({ pluginId: 'com.bedcode.alpha', name: 'Alpha' })
    // 名称排序上落后，但网络资源是始终允许 → 必须排第一（只看单资源的实现会漏）
    const midLoose = app({
      pluginId: 'com.bedcode.mid',
      name: 'Mid',
      strategies: [strategy('fs', 'default'), strategy('network', 'always_allow')],
    })

    expect(sortAppsByRisk([zetaStrict, alphaDefault, midLoose]).map((a) => a.name)).toEqual([
      'Mid',
      'Alpha',
      'Zeta',
    ])
  })

  it('同档按名称升序、同名按 pluginId 兜底，顺序确定不随入参顺序变', () => {
    const b = app({ pluginId: 'com.bedcode.b', name: 'Beta' })
    const a = app({ pluginId: 'com.bedcode.a', name: 'Alpha' })
    const a2 = app({ pluginId: 'com.bedcode.a2', name: 'Alpha' })

    expect(sortAppsByRisk([b, a, a2]).map((x) => x.pluginId)).toEqual([
      'com.bedcode.a',
      'com.bedcode.a2',
      'com.bedcode.b',
    ])
    expect(sortAppsByRisk([a2, a, b]).map((x) => x.pluginId)).toEqual([
      'com.bedcode.a',
      'com.bedcode.a2',
      'com.bedcode.b',
    ])
  })

  it('不修改入参数组（读模型是共享响应式状态的快照）', () => {
    const loose = app({
      pluginId: 'com.bedcode.loose',
      name: 'Loose',
      strategies: [strategy('fs', 'always_allow')],
    })
    const strict = app({ pluginId: 'com.bedcode.strict', name: 'Strict' })
    const input = [strict, loose]

    const sorted = sortAppsByRisk(input)

    expect(sorted.map((x) => x.pluginId)).toEqual(['com.bedcode.loose', 'com.bedcode.strict'])
    expect(input.map((x) => x.pluginId)).toEqual(['com.bedcode.strict', 'com.bedcode.loose'])
  })
})

// ==================== 四分区（票 07：详情页「授权记录」区块，spec §9.2） ====================

describe('授权记录四分区（票 07）', () => {
  it('按来源 + 效果分区：user/legacy → 用户已授权；always_allow → 免询问自动放行；deny → 硬拒绝', () => {
    const overview = app({
      pluginId: 'com.bedcode.test',
      records: [
        record({ id: 1, resource: 'fs', source: 'user' }),
        record({ id: 2, resource: 'fs', source: 'legacy' }),
        record({ id: 3, resource: 'fs', source: 'always_allow' }),
        record({ id: 4, resource: 'network', source: 'user_deny', effect: 'deny' }),
      ],
    })

    expect(userGrantedRecords(overview).map((r) => r.id)).toEqual([1, 2])
    expect(autoAllowedRecords(overview).map((r) => r.id)).toEqual([3])
    expect(deniedRecords(overview).map((r) => r.id)).toEqual([4])
  })

  it('未知来源的 allow 记录归「用户已授权」（保守口径，不误标成免询问自动放行）', () => {
    const overview = app({
      pluginId: 'com.bedcode.test',
      records: [record({ id: 1, resource: 'fs', source: 'always_allow_v2' })],
    })

    expect(userGrantedRecords(overview).map((r) => r.id)).toEqual([1])
    expect(autoAllowedRecords(overview)).toEqual([])
  })

  it('分区结果按落账时间稳定排序（同分区不因读模型顺序抖动）', () => {
    const overview = app({
      pluginId: 'com.bedcode.test',
      records: [
        record({ id: 2, resource: 'fs', source: 'user', createdAt: 20 }),
        record({ id: 1, resource: 'fs', source: 'user', createdAt: 10 }),
        record({ id: 4, resource: 'fs', effect: 'deny', source: 'user_deny', createdAt: 40 }),
        record({ id: 3, resource: 'fs', effect: 'deny', source: 'user_deny', createdAt: 30 }),
      ],
    })

    expect(userGrantedRecords(overview).map((r) => r.id)).toEqual([1, 2])
    expect(deniedRecords(overview).map((r) => r.id)).toEqual([3, 4])
  })

  it('firstPartyLabel：home 形态显示 ~/ 前缀，project-segment 显示 <project>/ 前缀', () => {
    expect(
      firstPartyLabel({ pluginId: 'com.bedcode.agent-hub', kind: 'home', value: '.agents' }),
    ).toBe('~/.agents')
    expect(
      firstPartyLabel({
        pluginId: 'com.bedcode.terminal-session',
        kind: 'project-segment',
        value: '.claude',
      }),
    ).toBe('<project>/.claude')
  })
})
