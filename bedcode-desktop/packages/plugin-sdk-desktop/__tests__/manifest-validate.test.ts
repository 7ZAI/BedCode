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
