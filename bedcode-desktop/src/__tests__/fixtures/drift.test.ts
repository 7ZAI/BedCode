/**
 * DTO 漂移对齐回归测试
 *
 * 对全部 fixtures 工厂做两件事：
 * 1. 逐一调用全部工厂，确保产出通过工厂内 assertDtoFields 键集断言
 *    （键集断言的唯一验证点：工厂被业务测试消费时随产出执行，
 *    未被消费时由本回归保证执行；Rust 新增字段后清单被同步而 fixture
 *    未更新 → 引用该 fixture 的任何测试立刻失败，显式告警）
 * 2. 断言 assertDtoFields 机制自身不退化（字段缺失/多余时必抛错）
 *
 * 2026-09-21：配对 / QR / 连接历史域随宿主命令面注销退役（产品面归
 * `com.bedcode.session` 插件），对应 fixtures（pairing.ts）与登记项一并删除。
 */

import { describe, it, expect } from 'vitest'
import { assertDtoFields } from './drift'
import {
  makeServerStatusInfo,
  makeNetworkConfig,
  makeServerMetrics,
  SERVER_STATUS_INFO_DTO_FIELDS,
  NETWORK_CONFIG_DTO_FIELDS,
  SERVER_METRICS_DTO_FIELDS,
} from './server'
import {
  makeSessionInfo,
  makeSessionConfig,
  makeWslDistro,
  makeDeviceConnectionInfo,
  SESSION_INFO_DTO_FIELDS,
  SESSION_CONFIG_DTO_FIELDS,
  WSL_DISTRO_DTO_FIELDS,
  DEVICE_CONNECTION_INFO_DTO_FIELDS,
} from './session'
import {
  makePluginInfo,
  makePluginContributes,
  PLUGIN_INFO_DTO_FIELDS,
  CONTRIBUTES_DTO_FIELDS,
} from './plugin'
import { makeAppConfig, APP_CONFIG_DTO_FIELDS } from './settings'

/** 全部 DTO 工厂注册表：drift 回归的单一清单（新增 fixture 必须在此登记） */
const DTO_REGISTRY = [
  {
    label: 'ServerStatusInfo',
    fields: SERVER_STATUS_INFO_DTO_FIELDS,
    build: () => makeServerStatusInfo(),
  },
  { label: 'NetworkConfig', fields: NETWORK_CONFIG_DTO_FIELDS, build: () => makeNetworkConfig() },
  { label: 'ServerMetrics', fields: SERVER_METRICS_DTO_FIELDS, build: () => makeServerMetrics() },
  { label: 'SessionInfo', fields: SESSION_INFO_DTO_FIELDS, build: () => makeSessionInfo() },
  { label: 'SessionConfig', fields: SESSION_CONFIG_DTO_FIELDS, build: () => makeSessionConfig() },
  { label: 'WslDistro', fields: WSL_DISTRO_DTO_FIELDS, build: () => makeWslDistro() },
  {
    label: 'DeviceConnectionInfo',
    fields: DEVICE_CONNECTION_INFO_DTO_FIELDS,
    build: () => makeDeviceConnectionInfo(),
  },
  { label: 'PluginInfo', fields: PLUGIN_INFO_DTO_FIELDS, build: () => makePluginInfo() },
  {
    label: 'PluginContributes',
    fields: CONTRIBUTES_DTO_FIELDS,
    build: () => makePluginContributes(),
  },
  { label: 'AppConfig', fields: APP_CONFIG_DTO_FIELDS, build: () => makeAppConfig() },
] as const

describe('fixtures DTO 对齐', () => {
  it.each(DTO_REGISTRY)(
    '$label 工厂产出通过键集断言（键集验证点在工厂内 assertDtoFields）',
    ({ label, build }) => {
      expect(() => build(), label).not.toThrow()
    },
  )

  it('makePluginInfo 产出时已断言嵌套 contributes 键集合', () => {
    expect(() => makePluginInfo()).not.toThrow()
  })

  it('factory override 不破坏键集合完整性（漂移时工厂自身抛错）', () => {
    // 覆盖部分字段后键集合不变，工厂内 assertDtoFields 仍然通过
    expect(() => makeServerStatusInfo({ port: 8766, uptime_secs: 42 })).not.toThrow()
    expect(() => makeSessionInfo({ id: 'session-override' })).not.toThrow()
  })
})

describe('assertDtoFields 机制自检', () => {
  /** 完整样本（工厂已断言键集）与其可删除字段 */
  const complete = makeSessionInfo() as unknown as Record<string, unknown>

  it('字段缺失时抛错（模拟 Rust 新增字段后 fixture 未同步）', () => {
    const missingKey = Object.keys(complete)[0]
    const stale = { ...complete }
    delete stale[missingKey]
    expect(() => assertDtoFields(stale, SESSION_INFO_DTO_FIELDS, 'SessionInfo')).toThrow(
      new RegExp(`漂移.*${missingKey}`),
    )
  })

  it('字段多余时抛错（模拟 fixture 含非线协议字段）', () => {
    const extra = { ...complete, phantom: 1 }
    expect(() => assertDtoFields(extra, SESSION_INFO_DTO_FIELDS, 'SessionInfo')).toThrow(
      /漂移.*phantom/,
    )
  })

  it('键集合一致时通过', () => {
    expect(() =>
      assertDtoFields(makeSessionInfo(), SESSION_INFO_DTO_FIELDS, 'SessionInfo'),
    ).not.toThrow()
  })
})
