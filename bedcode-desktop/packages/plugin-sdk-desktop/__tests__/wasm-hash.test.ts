/**
 * wasm-hash 的产物摘要注入与分发复核（审计票 14）
 *
 * 行为契约（来源：票 14 裁决 + bin/wasm-hash.js 分支）：
 * - C-W1 摘要只写**产物** plugin.json（裁决 A）：值 = 产物内 `<rustLibrary>.wasm` 字节的
 *   SHA-256，形态小写 64 位十六进制；源清单不参与（源不带该键，构建后 git status 干净）。
 * - C-W2 无 `rustLibrary`（纯前端插件 / 空串）→ no-op，且**不写文件**（无 wasm 可摘要）。
 * - C-W3 声明了 `rustLibrary` 却找不到对应 wasm、产物缺 plugin.json、清单非法 JSON
 *   → 一律抛错（fail-visible）：宿主安装端对同样三种情形都拒装，构建期就该报，
 *   且抛错时**不得**留下半写产物。
 * - C-W4 幂等：字节未变时再注入不落盘（避免每次构建刷 mtime / 制造无意义 diff）。
 * - C-W5 陈旧即覆盖：wasm 改一字节后重注入，摘要随新字节变（这条链存在的全部意义）。
 * - C-W6 摘要对象唯一：只取 `<rustLibrary>.wasm`，同目录其它 .wasm 是诱饵不参与（与宿主
 *   `{rust_library}{WASM_FILE_EXT}` 的取文件规则同源）。
 * - C-W7 写回形状稳定：保留其余字段与 2 空格缩进 + 末尾换行（产物 manifest 仍可读、可比对）。
 * - C-W8 verifyWasmHash 四态：一致 ok / 缺键 / 形态非法 / 字节失配 / 缺文件 → 各自可操作错误；
 *   纯前端产物视为通过。
 * - C-W9 跨语言对齐：与宿主 `manager/downloader.rs` 共用一条已知答案向量（同一字节串、
 *   同一算法、同一形态），改任一侧即两侧转红。
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  WASM_HASH_PATTERN,
  injectWasmHash,
  sha256Hex,
  verifyWasmHash,
  wasmFileName,
} from '../bin/wasm-hash.js'

// ==================== 测试夹具 ====================

let cwd: string

/** C-W9 / C-W1 的已知答案向量：wasm 魔数头 8 字节，宿主侧 downloader.rs 断言同一个十六进制串 */
const WASM_BYTES = Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00])
const WASM_BYTES_SHA256 = '93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476'
/** 末位改 1 后的向量（C-W5 陈旧覆盖）与诱饵文件向量（C-W6），均由独立实现算出后钉死 */
const TAMPERED_SHA256 = '3f499bf4c9e7483e804244d5e485b3537b2135690a7ce7b3fd7cb2544217d729'
const DECOY_SHA256 = '8e2632c5a345c8b88bbd42865e29508f353a5dbbd8f2bdaed7a19656898d7e5b'
/** 单字节 0x00 的向量（C-W8 失配断言里 manifest/actual 两值的 actual 侧） */
const ZERO_SHA256 = '6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d'

/** 搭建产物目录：manifest 可覆盖，wasmFiles 是「文件名 → 字节」 */
function scaffold(
  manifest: Record<string, unknown>,
  wasmFiles: Record<string, Buffer> = { 'test_plugin.wasm': WASM_BYTES },
): string {
  mkdirSync(cwd, { recursive: true })
  writeFileSync(join(cwd, 'plugin.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf-8')
  for (const [name, bytes] of Object.entries(wasmFiles)) {
    writeFileSync(join(cwd, name), bytes)
  }
  return cwd
}

function baseManifest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'com.example.test',
    name: 'Test Plugin',
    version: '1.0.0',
    main: 'index.js',
    pluginType: 'rust-ts',
    rustLibrary: 'test_plugin',
    permissions: [],
    contributes: {},
    ...overrides,
  }
}

/** 纯前端插件（ts-only 无 rustLibrary）——JSON 序列化时 undefined 的键不落盘 */
const TS_ONLY_MANIFEST = baseManifest({ pluginType: 'ts-only', rustLibrary: undefined })

const readManifest = (): Record<string, unknown> =>
  JSON.parse(readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
const readManifestRaw = (): string => readFileSync(join(cwd, 'plugin.json'), 'utf-8')

beforeEach(() => {
  cwd = mkdtempSync(join(tmpdir(), 'wasm-hash-'))
})

afterEach(() => {
  rmSync(cwd, { recursive: true, force: true })
})

// ==================== 契约 ====================

describe('injectWasmHash · C-W1 注入产物摘要', () => {
  it('C-W1 声明 rustLibrary 时写入该 wasm 字节的已知答案摘要', () => {
    const dir = scaffold(baseManifest())
    const result = injectWasmHash(dir)

    expect(result).toEqual({
      injected: true,
      wasmFile: 'test_plugin.wasm',
      hash: WASM_BYTES_SHA256,
      reason: 'written',
    })
    expect(readManifest().wasmHash).toBe(WASM_BYTES_SHA256)
  })

  it('C-W1 产出形态为小写 64 位十六进制', () => {
    injectWasmHash(scaffold(baseManifest(), { 'test_plugin.wasm': WASM_BYTES }))
    expect(String(readManifest().wasmHash)).toMatch(WASM_HASH_PATTERN)
  })
})

describe('injectWasmHash · C-W2 纯前端插件 no-op', () => {
  it('C-W2 无 rustLibrary（纯前端插件）→ 不写文件、判为 no-op', () => {
    const dir = scaffold(TS_ONLY_MANIFEST, {})
    const before = readManifestRaw()

    expect(injectWasmHash(dir)).toEqual({
      injected: false,
      wasmFile: null,
      hash: null,
      reason: 'no-rust-library',
    })
    expect(readManifestRaw()).toBe(before)
  })

  it('C-W2 rustLibrary 为空白串等同未声明', () => {
    const dir = scaffold(baseManifest({ rustLibrary: '   ' }))
    expect(injectWasmHash(dir).reason).toBe('no-rust-library')
    expect(readManifest().wasmHash).toBeUndefined()
  })

  it('C-W2 wasmFileName 判据：未声明返回 null，声明则加 .wasm 后缀', () => {
    expect(wasmFileName({ pluginType: 'ts-only' })).toBeNull()
    expect(wasmFileName({ rustLibrary: 'a_b' })).toBe('a_b.wasm')
  })
})

describe('injectWasmHash · C-W3 fail-visible 异常路径', () => {
  it('C-W3 声明了 rustLibrary 却缺 wasm → 抛错点名文件且不留半写产物', () => {
    const dir = scaffold(baseManifest(), {})

    expect(() => injectWasmHash(dir)).toThrowError(/声明 rustLibrary=test_plugin 但产物缺少 test_plugin\.wasm/)
    expect(readManifest().wasmHash).toBeUndefined()
  })

  it('C-W3 产物缺 plugin.json → 抛错', () => {
    mkdirSync(cwd, { recursive: true })
    expect(() => injectWasmHash(cwd)).toThrowError(/产物目录缺少 plugin\.json/)
  })

  it('C-W3 产物 plugin.json 不是合法 JSON → 抛错', () => {
    const dir = scaffold(baseManifest())
    writeFileSync(join(dir, 'plugin.json'), '{ not json', 'utf-8')
    expect(() => injectWasmHash(dir)).toThrowError(/不是合法 JSON/)
  })
})

describe('injectWasmHash · C-W4 幂等 / C-W5 陈旧覆盖 / C-W6 摘要对象唯一', () => {
  it('C-W4 字节未变时再注入不落盘（幂等，reason=unchanged）', () => {
    const dir = scaffold(baseManifest())
    injectWasmHash(dir)
    const after = readManifestRaw()

    expect(injectWasmHash(dir).reason).toBe('unchanged')
    expect(readManifestRaw()).toBe(after)
  })

  it('C-W5 wasm 改一字节后重注入 → 陈旧摘要被新字节覆盖', () => {
    const dir = scaffold(baseManifest())
    injectWasmHash(dir)
    const first = String(readManifest().wasmHash)

    const tampered = Buffer.from(WASM_BYTES)
    tampered[tampered.length - 1] = 0x01
    writeFileSync(join(dir, 'test_plugin.wasm'), tampered)

    const result = injectWasmHash(dir)
    expect(result.reason).toBe('written')
    expect(result.hash).not.toBe(first)
    expect(result.hash).toBe(TAMPERED_SHA256)
    expect(readManifest().wasmHash).toBe(TAMPERED_SHA256)
  })

  it('C-W6 同目录存在诱饵 .wasm 时只摘要声明的那个文件', () => {
    const decoy = Buffer.from([0xff, 0xee, 0xdd])
    const dir = scaffold(baseManifest(), {
      'test_plugin.wasm': WASM_BYTES,
      'other_plugin.wasm': decoy,
      'bedcode_plugin_old.wasm': Buffer.concat([WASM_BYTES, WASM_BYTES]),
    })

    // 诱饵自身的摘要（独立实现算出）——若实现误取诱饵即转红
    expect(injectWasmHash(dir).hash).toBe(WASM_BYTES_SHA256)
    expect(sha256Hex(decoy)).toBe(DECOY_SHA256)
    expect(injectWasmHash(dir).hash).not.toBe(DECOY_SHA256)
  })
})

describe('injectWasmHash · C-W7 写回形状稳定', () => {
  it('C-W7 写回保留其余字段、2 空格缩进与末尾换行', () => {
    const dir = scaffold(baseManifest({ description: 'keep me' }))
    injectWasmHash(dir)
    const raw = readManifestRaw()

    expect(raw.endsWith('\n')).toBe(true)
    expect(raw.includes('\n  "id": "com.example.test",')).toBe(true)
    expect(readManifest()).toMatchObject({
      id: 'com.example.test',
      description: 'keep me',
      rustLibrary: 'test_plugin',
      wasmHash: WASM_BYTES_SHA256,
    })
  })
})

describe('verifyWasmHash · C-W8 分发链复核四态', () => {
  it('C-W8 字节与摘要一致 → 通过', () => {
    const dir = scaffold(baseManifest())
    injectWasmHash(dir)
    expect(verifyWasmHash(dir)).toEqual({ ok: true, hash: WASM_BYTES_SHA256 })
  })

  it('C-W8 纯前端产物无摘要也视为通过', () => {
    const dir = scaffold(TS_ONLY_MANIFEST, {})
    expect(verifyWasmHash(dir)).toEqual({ ok: true, hash: null })
  })

  it('C-W8 缺 wasmHash → 失败并指向重新构建', () => {
    const dir = scaffold(baseManifest())
    expect(verifyWasmHash(dir)).toEqual({
      ok: false,
      error: expect.stringMatching(/产物缺少 wasmHash.*重新构建/s),
    })
  })

  it('C-W8 形态非法 → 失败并点明小写 64 位十六进制', () => {
    const dir = scaffold(baseManifest({ wasmHash: 'DEADBEEF' }))
    expect(verifyWasmHash(dir).ok).toBe(false)
    expect(String(verifyWasmHash(dir).error)).toContain('须为小写 64 位十六进制')
  })

  it('C-W8 字节与清单失配 → 失败且同时给出 manifest/actual 两个摘要', () => {
    const dir = scaffold(baseManifest())
    injectWasmHash(dir)
    writeFileSync(join(dir, 'test_plugin.wasm'), Buffer.from([0x00]))

    const res = verifyWasmHash(dir)
    expect(res.ok).toBe(false)
    expect(String(res.error)).toContain(`manifest=${WASM_BYTES_SHA256}`)
    expect(String(res.error)).toContain(`actual=${ZERO_SHA256}`)
  })

  it('C-W8 摘要合法但 wasm 文件缺失 → 失败点名缺文件', () => {
    const dir = scaffold(baseManifest({ wasmHash: WASM_BYTES_SHA256 }), {})
    expect(verifyWasmHash(dir)).toEqual({
      ok: false,
      error: expect.stringContaining('产物缺少 test_plugin.wasm'),
    })
  })
})

describe('sha256Hex · C-W9 与宿主下载端同一向量', () => {
  it('C-W9 与宿主 downloader.rs 断言同一条已知答案向量', () => {
    expect(sha256Hex(WASM_BYTES)).toBe(WASM_BYTES_SHA256)
  })
})
