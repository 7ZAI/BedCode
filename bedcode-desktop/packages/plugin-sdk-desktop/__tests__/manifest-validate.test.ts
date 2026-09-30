/**
 * manifest-validate 的 `contributes.httpEndpoints` 与 `wasmHash` 规则（票 16 立、票 08 追加档位、票 14 追加摘要）
 *
 * 行为契约（来源：票面 + bin/manifest-validate.js 分支）：
 * - C-V1 条目两形态并存：纯路径段 / `{path, auth}`，都合法；
 * - C-V2 `auth` 只认 `none | jwt`（真源 SDK rust/src/types.rs 的 EndpointAuth），
 *   写错即构建失败——宿主侧对非法档位的处置是**不登记该端点**（静默不可达），
 *   所以构建期必须拦在打包前；
 * - C-V3 形态错误（数字 / null / 数组 / 未知字段）不得降级成「未声明该端点」；
 * - C-V4 path 判据沿用票 16：非空、不含 `..`；前导斜杠归一后参与重复检测，
 *   因此 `"x"` 与 `{path:"/x"}` 视为同一条（宿主登记出的全路径相同）；
 * - C-V5 `wasmHash` 形态必须是已定义的小写 64 位十六进制 SHA-256（正则真源在 `bin/wasm-hash.js`，
 *   此处不手抄）——非法值混进产物会让宿主安装端拒装，构建期即拦；
 * - C-V6 该字段由构建注入**产物**（票 14 裁决 A），源清单写了不会被刷新 → 只告警不报错；
 *   缺省与空串都是合法态（宿主 `wasm_hash.trim().is_empty()` = 未声明，跳过比对）。
 *
 * `wasiPreopenDirs` 规则（票 07 只读档）：
 * - C-W1 条目两形态并存：裸路径字符串（可写，既有形态）/ `{path, readonly}`，都合法；
 *   `readonly` 缺省 = 可写，所以既有 manifest 零迁移；
 * - C-W2 缺省 / null / 空数组都是合法态（无预打开目录）；
 * - C-W3 形态错误（数字 / null / 数组 / path 空或全空白）不得降级成「未声明该目录」；
 * - C-W4 `readonly` 非布尔（`"true"` / `1`）即构建失败——宿主 Rust 侧同样拒绝，
 *   两侧都不允许「静默降级为可写」；
 * - C-W5 未知键（`read_only` 这类拼写）报错并点名该键与允许字段。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { validateManifest } from '../bin/manifest-validate.js'

let cwd: string

/** 写一份只关心 httpEndpoints 的最小 manifest，返回其中与 httpEndpoints 相关的错误 */
function errorsForHttpEndpoints(httpEndpoints: unknown): string[] {
  const contributes =
    httpEndpoints === undefined ? {} : { httpEndpoints: httpEndpoints as never }
  writeFileSync(
    join(cwd, 'plugin.json'),
    JSON.stringify(
      {
        id: 'com.example.test',
        name: 'Test Plugin',
        version: '1.0.0',
        main: 'index.js',
        pluginType: 'rust-ts',
        rustLibrary: 'test_plugin',
        permissions: [],
        contributes,
      },
      null,
      2,
    ),
    'utf-8',
  )
  const { errors } = validateManifest(cwd)
  return errors.filter((e) => e.includes('httpEndpoints'))
}

beforeEach(() => {
  cwd = mkdtempSync(join(tmpdir(), 'manifest-validate-'))
})

afterEach(() => {
  rmSync(cwd, { recursive: true, force: true })
})

describe('contributes.httpEndpoints 校验', () => {
  it('C-V1 纯路径段与带 auth 的对象形态都合法', () => {
    expect(errorsForHttpEndpoints(['configs', 'task-status'])).toEqual([])
    expect(
      errorsForHttpEndpoints([
        { path: 'task-status', auth: 'none' },
        { path: 'task-queue/add', auth: 'jwt' },
        { path: 'session-mode' },
      ]),
    ).toEqual([])
  })

  it('C-V1 缺省与 null 都是「未声明」，不报错（宿主侧即没有 HTTP 面）', () => {
    expect(errorsForHttpEndpoints(undefined)).toEqual([])
    expect(errorsForHttpEndpoints(null)).toEqual([])
    expect(errorsForHttpEndpoints([])).toEqual([])
  })

  it('C-V2 auth 只认 none | jwt，非法取值点明合法档位与缺省方向', () => {
    const errors = errorsForHttpEndpoints([{ path: 'task-status', auth: 'local-only' }])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('local-only')
    expect(errors[0]).toContain('none')
    expect(errors[0]).toContain('jwt')
    // 大小写敏感（与 Rust 解析一致）：JWT 不是合法档位
    expect(errorsForHttpEndpoints([{ path: 'a', auth: 'JWT' }])).toHaveLength(1)
    // 空串不是合法档位——它不是「缺省」，写出来即错
    expect(errorsForHttpEndpoints([{ path: 'a', auth: '' }])).toHaveLength(1)
  })

  it('C-V3 条目形态非法必须报错，不静默当成未声明条目', () => {
    for (const bad of [42, null, true, ['task-status']]) {
      expect(errorsForHttpEndpoints([bad]), `条目 ${JSON.stringify(bad)} 应被拒`).toHaveLength(1)
    }
    // 合法对象但缺 path：一条「path 非法」+ 一条「未知字段」，两条都要报（方向不能含糊）
    expect(errorsForHttpEndpoints([{ notPath: 'x' }])).toHaveLength(2)
  })

  it('C-V3 对象条目的未知字段被拒（防止 method/regex 之类被误当成宿主判据）', () => {
    const errors = errorsForHttpEndpoints([{ path: 'task-status', method: 'GET' }])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('method')
  })

  it('C-V4 path 判据：空段与 .. 被拒，且 path 必须是字符串', () => {
    expect(errorsForHttpEndpoints([{ path: '   ' }])).toHaveLength(1)
    expect(errorsForHttpEndpoints([{ path: '../secrets' }])).toHaveLength(1)
    expect(errorsForHttpEndpoints([{ path: 1 }])).toHaveLength(1)
    // 纯字符串条目的既有判据不回退
    expect(errorsForHttpEndpoints([''])).toHaveLength(1)
    expect(errorsForHttpEndpoints(['a/../b'])).toHaveLength(1)
  })

  it('C-V4 前导斜杠归一后参与重复检测（两形态同一路径即重复）', () => {
    const dupAcrossForms = errorsForHttpEndpoints(['task-status', { path: '/task-status' }])
    expect(dupAcrossForms).toHaveLength(1)
    expect(dupAcrossForms[0]).toContain('重复声明')
    expect(dupAcrossForms[0]).toContain('task-status')
    // 同一路径只出现一次 → 不报重复
    expect(errorsForHttpEndpoints(['task-status', { path: 'session-mode', auth: 'none' }])).toEqual([])
  })

  it('非数组整体被拒且给出两形态的正确写法', () => {
    const errors = errorsForHttpEndpoints('task-status')
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('path')
    expect(errors[0]).toContain('auth')
  })
})

// ==================== contributes.wsEndpoints（票 09a）====================

/** 写一份只关心 wsEndpoints 的最小 manifest，返回与 wsEndpoints 相关的错误 */
function errorsForWsEndpoints(wsEndpoints: unknown): string[] {
  const contributes =
    wsEndpoints === undefined ? {} : { wsEndpoints: wsEndpoints as never }
  writeFileSync(
    join(cwd, 'plugin.json'),
    JSON.stringify(
      {
        id: 'com.example.test',
        name: 'Test Plugin',
        version: '1.0.0',
        main: 'index.js',
        pluginType: 'rust-ts',
        rustLibrary: 'test_plugin',
        permissions: [],
        contributes,
      },
      null,
      2,
    ),
    'utf-8',
  )
  const { errors } = validateManifest(cwd)
  return errors.filter((e) => e.includes('wsEndpoints'))
}

describe('contributes.wsEndpoints 校验（票 09a WS 动作词表声明式化）', () => {
  it('W-V1 纯路径段与带 auth 的对象形态都合法（同 httpEndpoints 两形态）', () => {
    expect(errorsForWsEndpoints(['echo', 'status'])).toEqual([])
    expect(
      errorsForWsEndpoints([
        { path: 'echo', auth: 'none' },
        { path: 'chat', auth: 'jwt' },
        { path: 'session-mode' },
      ]),
    ).toEqual([])
  })

  it('W-V1 缺省与 null 都是「未声明」，不报错（未声明清单 = 插件 WS 面不可达）', () => {
    expect(errorsForWsEndpoints(undefined)).toEqual([])
    expect(errorsForWsEndpoints(null)).toEqual([])
    expect(errorsForWsEndpoints([])).toEqual([])
  })

  it('W-V2 auth 只认 none | jwt，非法取值点明合法档位与 WS 缺省方向', () => {
    const errors = errorsForWsEndpoints([{ path: 'echo', auth: 'token' }])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('token')
    expect(errors[0]).toContain('none')
    expect(errors[0]).toContain('jwt')
    // 大小写敏感（与 Rust 解析一致）：JWT 不是合法档位
    expect(errorsForWsEndpoints([{ path: 'a', auth: 'JWT' }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: 'a', auth: '' }])).toEqual([])
    expect(errorsForWsEndpoints([{ path: 'b', auth: null }])).toEqual([])
    expect(errorsForWsEndpoints([{ path: 'c', auth: ' none ' }])).toEqual([])
  })

  it('W-V3 条目形态非法必须报错，不静默当成未声明条目', () => {
    for (const bad of [42, null, true, ['echo']]) {
      expect(errorsForWsEndpoints([bad]), `条目 ${JSON.stringify(bad)} 应被拒`).toHaveLength(1)
    }
    // 合法对象但缺 path：一条「path 非法」+ 一条「未知字段」，两条都要报
    expect(errorsForWsEndpoints([{ notPath: 'x' }])).toHaveLength(2)
  })

  it('W-V3 对象条目的未知字段被拒（WS 端点只认 path / auth）', () => {
    const errors = errorsForWsEndpoints([{ path: 'echo', method: 'GET' }])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('method')
  })

  it('W-V4 path 判据：空段、分隔符、点段与超长路径被拒，且 path 必须是字符串', () => {
    expect(errorsForWsEndpoints([{ path: '   ' }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: '../secrets' }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: 'a/b' }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: 'a.b' }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: 'x'.repeat(65) }])).toHaveLength(1)
    expect(errorsForWsEndpoints([{ path: 1 }])).toHaveLength(1)
    expect(errorsForWsEndpoints([''])).toHaveLength(1)
  })

  it('W-V4 两形态逐字同路径才参与重复检测，分隔路径直接拒绝', () => {
    const duplicate = errorsForWsEndpoints(['echo', { path: 'echo' }])
    expect(duplicate).toHaveLength(1)
    expect(duplicate[0]).toContain('重复声明')
    expect(errorsForWsEndpoints(['echo', { path: '/echo' }])).toHaveLength(1)
    expect(errorsForWsEndpoints(['echo', { path: 'status', auth: 'none' }])).toEqual([])
  })

  it('非数组整体被拒且给出两形态的正确写法', () => {
    const errors = errorsForWsEndpoints('echo')
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('path')
    expect(errors[0]).toContain('auth')
  })
})

// ==================== wasmHash（票 14）====================

/** 写一份只关心 wasmHash 的最小 manifest，返回与该字段相关的 errors / warnings */
function outcomeForWasmHash(wasmHash: unknown): { errors: string[]; warnings: string[] } {
  const manifest: Record<string, unknown> = {
    id: 'com.example.test',
    name: 'Test Plugin',
    version: '1.0.0',
    main: 'index.js',
    pluginType: 'rust-ts',
    rustLibrary: 'test_plugin',
    permissions: [],
    contributes: {},
  }
  if (wasmHash !== undefined) manifest.wasmHash = wasmHash
  writeFileSync(join(cwd, 'plugin.json'), JSON.stringify(manifest, null, 2), 'utf-8')
  const { errors, warnings } = validateManifest(cwd)
  return {
    errors: errors.filter((e) => e.includes('wasmHash')),
    warnings: warnings.filter((w) => w.includes('wasmHash')),
  }
}

describe('wasmHash 校验', () => {
  it('C-V5 形态非法（长度不足 / 含非 hex / 非字符串）即构建失败', () => {
    expect(outcomeForWasmHash('deadbeef').errors).toHaveLength(1)
    expect(outcomeForWasmHash('DEADBEEF').errors).toHaveLength(1)
    expect(outcomeForWasmHash(`${'z'.repeat(64)}`).errors).toHaveLength(1)
    expect(outcomeForWasmHash(123).errors).toHaveLength(1)
    expect(outcomeForWasmHash({}).errors).toHaveLength(1)
    expect(outcomeForWasmHash(`${'a'.repeat(63)}b`).errors).toEqual([])
  })

  it('C-V6 源清单带该键时给可操作警告（值由构建注入产物，手写不会被刷新）', () => {
    const { errors, warnings } = outcomeForWasmHash(`${'a'.repeat(64)}`)
    expect(errors).toEqual([])
    expect(warnings).toHaveLength(1)
    expect(warnings[0]).toContain('注入产物')
  })

  it('C-V6 缺省（源清单的正常形态）既不报错也不告警', () => {
    const { errors, warnings } = outcomeForWasmHash(undefined)
    expect(errors).toEqual([])
    expect(warnings).toEqual([])
  })

  it('C-V6 空串视为未声明（与宿主 downloader 的 trim().is_empty() 判据同形）', () => {
    const { errors, warnings } = outcomeForWasmHash('')
    expect(errors).toEqual([])
    expect(warnings).toEqual([])
  })
})

// ==================== wasiPreopenDirs（票 07 只读档）====================

/** 写一份只关心 wasiPreopenDirs 的最小 manifest，返回与该字段相关的错误 */
function errorsForWasiPreopenDirs(dirs: unknown, lifecycle: string | null = 'ephemeral'): string[] {
  const manifest: Record<string, unknown> = {
    id: 'com.example.test',
    name: 'Test Plugin',
    version: '1.0.0',
    main: 'index.js',
    pluginType: 'rust-ts',
    rustLibrary: 'test_plugin',
    permissions: [],
    contributes: {},
  }
  // ADR 0034 后 preopen 仅 worker 类别可用：形态校验默认放在合法类别（ephemeral）
  // 语境下测，否则类别闸门错误会污染形态断言（ephemeral 本身被 lifecycle 分支拦截，
  // 其错误不含 'wasiPreopenDirs'，过滤后不影响形态断言）；传 null = 不写 lifecycle
  // （缺省 persistent），用于类别闸门反例
  if (lifecycle !== null) manifest.lifecycle = lifecycle
  if (dirs !== undefined) manifest.wasiPreopenDirs = dirs
  writeFileSync(join(cwd, 'plugin.json'), JSON.stringify(manifest, null, 2), 'utf-8')
  const { errors } = validateManifest(cwd)
  return errors.filter((e) => e.includes('wasiPreopenDirs'))
}

describe('wasiPreopenDirs 校验', () => {
  it('C-W1 裸路径与带 readonly 的对象形态都合法', () => {
    expect(errorsForWasiPreopenDirs(['${home}/.bedcode/ai-chatbox', '/srv/data'])).toEqual([])
    expect(
      errorsForWasiPreopenDirs([
        { path: '${home}/.ssh', readonly: true },
        { path: '${home}/write-me', readonly: false },
        { path: '${home}/no-flag' },
      ]),
    ).toEqual([])
  })

  it('C-W6 类别闸门（ADR 0034）：非 worker 声明 preopen 构建期拒绝，文案指路 host-fs', () => {
    // 缺省 lifecycle（= persistent）→ 类别闸门错误
    const defaultErrors = errorsForWasiPreopenDirs(['/x'], null)
    expect(defaultErrors).toHaveLength(1)
    expect(defaultErrors[0]).toContain('host-fs')
    expect(defaultErrors[0]).toContain('ephemeral')
    // 显式 persistent → 同样拒
    const persistentErrors = errorsForWasiPreopenDirs(['/x'], 'persistent')
    expect(persistentErrors).toHaveLength(1)
    expect(persistentErrors[0]).toContain('host-fs')
    // worker（ephemeral）声明 → 不报类别闸门错误（形态合法时零 wasiPreopenDirs 错误）
    expect(errorsForWasiPreopenDirs(['/x'], 'ephemeral')).toEqual([])
  })

  it('C-W6 类别闸门在形态校验之外独立生效：非法形态 + 非 worker 同时报两条', () => {
    const errors = errorsForWasiPreopenDirs('${home}/x', 'persistent')
    expect(errors).toHaveLength(2)
    expect(errors.some((e) => e.includes('host-fs'))).toBe(true)
    expect(errors.some((e) => e.includes('readonly'))).toBe(true)
  })

  it('C-W1 两形态混列合法（同一列表里档位逐条独立）', () => {
    expect(errorsForWasiPreopenDirs(['/a', { path: '/b', readonly: true }, '/c'])).toEqual([])
  })

  it('C-W2 缺省 / null / 空数组均为合法态（无预打开目录）', () => {
    expect(errorsForWasiPreopenDirs(undefined)).toEqual([])
    expect(errorsForWasiPreopenDirs(null)).toEqual([])
    expect(errorsForWasiPreopenDirs([])).toEqual([])
  })

  it('C-W2 该规则不越界：其它字段合法时零错误', () => {
    // 反例守卫：若过滤串写成 includes('preopen') 之类，合法 manifest 也会被判错
    expect(errorsForWasiPreopenDirs(['/x'])).toEqual([])
  })

  it('C-W3 整体非数组被拒，且错误文案给出两形态的正确写法', () => {
    const errors = errorsForWasiPreopenDirs('${home}/x')
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('path')
    expect(errors[0]).toContain('readonly')
  })

  it('C-W4 条目形态非法即拒，不得降级成「未声明该目录」', () => {
    for (const bad of [42, null, true, ['/x'], {}]) {
      const errors = errorsForWasiPreopenDirs(['/ok', bad])
      expect(errors.length, `非法条目 ${JSON.stringify(bad)} 必须报错`).toBeGreaterThan(0)
    }
  })

  it('C-W4 空 path / 全空白 path / 非字符串 path 各报一条', () => {
    expect(errorsForWasiPreopenDirs(['   '])).toHaveLength(1)
    expect(errorsForWasiPreopenDirs([''])).toHaveLength(1)
    expect(errorsForWasiPreopenDirs([{ path: 42 }])).toHaveLength(1)
    expect(errorsForWasiPreopenDirs([{ path: '' }])).toHaveLength(1)
  })

  it('C-W5 readonly 非布尔即拒：静默忽略会把只读声明降级成可写挂载', () => {
    for (const bad of ['true', 1, 'false', null, {}]) {
      const errors = errorsForWasiPreopenDirs([{ path: '/x', readonly: bad }])
      expect(errors.length, `readonly=${JSON.stringify(bad)} 必须报错`).toBeGreaterThan(0)
      expect(errors.some((e) => e.includes('readonly')), `错误文案须点名 readonly: ${errors}`).toBe(true)
    }
  })

  it('C-W5 未知键即拒并点名该键（read_only 这类拼错不得静默通过）', () => {
    const errors = errorsForWasiPreopenDirs([{ path: '/x', read_only: true }])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('read_only')
    expect(errors[0]).toContain('path')
    expect(errors[0]).toContain('readonly')
  })

  it('C-W4 合法条目混在非法条目里时，只报非法那一条', () => {
    const errors = errorsForWasiPreopenDirs([{ path: '/ok', readonly: true }, 7])
    expect(errors).toHaveLength(1)
    expect(errors[0]).toContain('7')
  })
})

/**
 * 分类字段（`type` / `lifecycle`，ADR 0032）校验
 *
 * 行为契约（真源：bin/manifest-validate.js 分支 + SDK rust/src/types.rs 的
 * `PluginKind` / `InstanceLifecycle`）：
 * - C-R1 `type` 取值域 = L1 `basic-service` / L2 `internal-business` / L3 `business-app`；
 *   缺省即 L3，不写 `type` 的既有工程零迁移；
 * - C-R2 非法取值**构建期拒**——宿主反序列化虽也拒，但那时包已发出去，
 *   现场表现是「插件装了却不出现」，构建期必须先拦；
 * - C-R3 历史拼写 `system` / `application` 宿主仍按别名接受（旧产物零迁移），
 *   故只告警不拦（拦会打断既有工程），且告警点名现行拼写；
 * - C-R4 `lifecycle` 缺省 / `persistent` 是合法态（常驻）；
 * - C-R5 `ephemeral` 本期只预留类型：宿主一次性实例机制与调度框架未落地，
 *   声明即在**构建期与宿主加载期双侧显性拒绝**——不静默当常驻处理
 *   （那会让作者以为 worker 生效，而常驻恰是 worker 存在理由的反面）；
 * - C-R6 `ephemeral` 必须配 `pluginType: rust`（无页面的即用即弃形态）。
 */
describe('分类字段 type / lifecycle 校验（ADR 0032）', () => {
  /** 写一份最小 manifest，返回与分类字段相关的错误 / 告警 */
  function classify(extra: Record<string, unknown>): { errors: string[]; warnings: string[] } {
    writeFileSync(
      join(cwd, 'plugin.json'),
      JSON.stringify(
        {
          id: 'com.example.test',
          name: 'Test Plugin',
          version: '1.0.0',
          main: 'index.js',
          pluginType: 'rust',
          rustLibrary: 'test_plugin',
          permissions: [],
          contributes: {},
          ...extra,
        },
        null,
        2,
      ),
      'utf-8',
    )
    const { errors, warnings } = validateManifest(cwd)
    const relevant = (list: string[]) => list.filter((e) => e.includes('type') || e.includes('lifecycle'))
    return { errors: relevant(errors), warnings: relevant(warnings) }
  }

  it('C-R1 三个角色拼写都合法，缺省（不写 type）也合法', () => {
    for (const kind of ['basic-service', 'internal-business', 'business-app']) {
      expect(classify({ type: kind }).errors, `type=${kind} 应当合法`).toEqual([])
    }
    expect(classify({}).errors, '缺省即 L3 业务应用，旧工程零迁移').toEqual([])
  })

  it('C-R2 非法角色取值构建期即拒，且文案点名允许值', () => {
    for (const bad of ['systemm', 'internal', 'BASIC-SERVICE', 'worker']) {
      const { errors } = classify({ type: bad })
      expect(errors.length, `type=${bad} 必须被拒`).toBeGreaterThan(0)
      expect(errors[0]).toContain(bad)
      expect(errors[0]).toContain('basic-service')
    }
  })

  it('C-R3 历史拼写只告警不拦（宿主按别名接受），且告警点名现行拼写', () => {
    for (const [legacy, current] of [
      ['system', 'basic-service'],
      ['application', 'business-app'],
    ]) {
      const { errors, warnings } = classify({ type: legacy })
      expect(errors, `type=${legacy} 不得拦（会打断既有工程）`).toEqual([])
      expect(warnings.some((w) => w.includes(legacy) && w.includes(current))).toBe(true)
    }
  })

  it('C-R4 缺省与显式 persistent 都是合法态', () => {
    expect(classify({}).errors).toEqual([])
    expect(classify({ lifecycle: 'persistent' }).errors).toEqual([])
  })

  it('C-R5 ephemeral 构建期被拒（调度框架未落地），文案点名 ADR 0032 与缺口', () => {
    const { errors } = classify({ lifecycle: 'ephemeral' })
    expect(errors.length).toBe(1)
    expect(errors[0]).toContain('ephemeral')
    expect(errors[0]).toContain('ADR 0032')
    expect(errors[0]).toContain('调度')
  })

  it('C-R6 ephemeral 配非 rust 形态时先报形态错（无页面的即用即弃形态）', () => {
    for (const pluginType of ['rust-ts', 'ts-only']) {
      const { errors } = classify({ pluginType, lifecycle: 'ephemeral' })
      expect(errors.length, `pluginType=${pluginType} + ephemeral 必须被拒`).toBe(1)
      expect(errors[0]).toContain('rust')
    }
  })

  it('C-R4 非法 lifecycle 取值构建期即拒并点名允许值', () => {
    const { errors } = classify({ lifecycle: 'forever' })
    expect(errors.length).toBe(1)
    expect(errors[0]).toContain('forever')
    expect(errors[0]).toContain('persistent')
  })
})
