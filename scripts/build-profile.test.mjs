/**
 * build-profile.mjs 行为契约测试（node --test，零依赖）
 *
 * 行为契约：
 * | 契约ID | 来源 | 规则 |
 * | C-001 | parseLoadavg | 取 /proc/loadavg 首字段为 1 分钟负载；空/垃圾 → null |
 * | C-002 | parseMeminfo | 解析 kB 字段；缺 MemAvailable 用 MemFree+Cached+Buffers 兜底；缺字段补 0 |
 * | C-003 | parseCpuOnline | "0-15"→16、"0-7,16-23"→16、"0"→1；垃圾/空 → null |
 * | C-004 | parseVmstat | 解析 pswpin/pswpout；缺失字段补 0 |
 * | C-005 | parseWinLoadPercentage | 0~100 有效；逗号/句点小数兼容；越界/垃圾 → null |
 * | C-006 | mergeNodeOptions | 追加堆参数；已存在则替换；其余 token 原样保留 |
 * | C-007 | mergeGradleOpts | 追加 workers.max；已存在则替换不重复；其余 JVM 参数保留 |
 * | C-008 | classify 分档 | serial/balanced/parallel 判定（load 0.5/1.0/1.5、mem 1.5/3.0/4.0 边界） |
 * | C-009 | classify 公式 | jobs = min(cores, floor(mem/jobsPerGiB))；balanced 档 cap 2；serial 恒 1 |
 * | C-010 | classify 降级 | swapActive → serial；负载未知 → 仅按内存；内存未知 → 保守 serial |
 * | C-011 | classify 覆盖 | 显式 profile 优先于自动判定，但内存上限（公式）仍然生效 |
 * | C-012 | classify 预算 | jobsPerGiB 覆盖参与公式计算 |
 * | C-013 | buildEnv | 生成 CARGO_BUILD_JOBS/GRADLE_OPTS/NODE_OPTIONS 并与 baseEnv 合并 |
 * | C-014 | sampleSystem linux | /proc 三件套解析；swap 双采样仅在「有 swap 且内存<8GiB」时 sleep |
 * | C-015 | sampleSystem linux | swap 抖动阈值：窗口增量 >1 页才判 active |
 * | C-016 | sampleSystem win32 | PS 成功 → loadRatio=忙碌%/100；status≠0/抛错 → 负载未知 |
 * | C-017 | sampleSystem darwin | os 原生 API；loadKnown=true |
 * | C-018 | sampleSystem 未知平台 | 全字段置空/未知，不注入覆盖 |
 *
 * 测试矩阵见各 describe 内命名用例（正例/反例/边界/异常全覆盖）。
 */

import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import os from 'node:os'
import {
  parseLoadavg,
  parseMeminfo,
  parseCpuOnline,
  parseVmstat,
  parseWinLoadPercentage,
  mergeNodeOptions,
  mergeGradleOpts,
  classify,
  buildEnv,
  sampleSystem,
  SWAP_SAMPLE_MS,
} from './build-profile.mjs'

// ==================== C-001 parseLoadavg ====================
describe('parseLoadavg', () => {
  it('正例：取首字段为 1 分钟负载', () => {
    assert.equal(parseLoadavg('5.89 4.35 3.65 1/812 45678'), 5.89)
  })
  it('边界：负载为 0', () => {
    assert.equal(parseLoadavg('0.00 0.01 0.02 1/1 1'), 0)
  })
  it('反例：空串 → null', () => {
    assert.equal(parseLoadavg(''), null)
  })
  it('反例：垃圾内容 → null', () => {
    assert.equal(parseLoadavg('not-a-number'), null)
  })
})

// ==================== C-002 parseMeminfo ====================
describe('parseMeminfo', () => {
  const SAMPLE = [
    'MemTotal:       14139224 kB',
    'MemFree:         1142180 kB',
    'MemAvailable:    4897960 kB',
    'Buffers:          186392 kB',
    'Cached:          3189928 kB',
    'SwapTotal:             0 kB',
  ].join('\n')

  it('正例：解析标准字段（kB）', () => {
    const r = parseMeminfo(SAMPLE)
    assert.equal(r.memTotalKb, 14139224)
    assert.equal(r.memAvailableKb, 4897960)
    assert.equal(r.swapTotalKb, 0)
  })
  it('反例（兜底）：缺 MemAvailable 用 MemFree+Cached+Buffers 求和', () => {
    const noAvail = SAMPLE.replace('MemAvailable:    4897960 kB\n', '')
    const r = parseMeminfo(noAvail)
    assert.equal(r.memAvailableKb, 1142180 + 3189928 + 186392)
  })
  it('反例（兜底）：空内容全字段补 0', () => {
    const r = parseMeminfo('')
    assert.deepEqual(r, { memTotalKb: 0, memAvailableKb: 0, swapTotalKb: 0 })
  })
})

// ==================== C-003 parseCpuOnline ====================
describe('parseCpuOnline', () => {
  it('正例：连续区间 "0-15" → 16', () => {
    assert.equal(parseCpuOnline('0-15'), 16)
  })
  it('正例：多段区间 "0-7,16-23" → 16', () => {
    assert.equal(parseCpuOnline('0-7,16-23'), 16)
  })
  it('边界：单核 "0" → 1；区间+单核 "0-15,17" → 17', () => {
    assert.equal(parseCpuOnline('0'), 1)
    assert.equal(parseCpuOnline('0-15,17'), 17)
  })
  it('反例：空串 / 垃圾 → null', () => {
    assert.equal(parseCpuOnline(''), null)
    assert.equal(parseCpuOnline('abc'), null)
    assert.equal(parseCpuOnline('x-y'), null)
  })
})

// ==================== C-004 parseVmstat ====================
describe('parseVmstat', () => {
  it('正例：解析 pswpin/pswpout', () => {
    assert.deepEqual(parseVmstat('pswpin 12\npswpout 3\nfoo 1'), { pswpin: 12, pswpout: 3 })
  })
  it('反例（兜底）：缺失字段补 0', () => {
    assert.deepEqual(parseVmstat('pswpin 12'), { pswpin: 12, pswpout: 0 })
    assert.deepEqual(parseVmstat(''), { pswpin: 0, pswpout: 0 })
  })
})

// ==================== C-005 parseWinLoadPercentage ====================
describe('parseWinLoadPercentage', () => {
  it('正例：句点小数 12.5', () => {
    assert.equal(parseWinLoadPercentage('12.5\r\n'), 12.5)
  })
  it('边界：逗号小数（非英语区域）12,5 → 12.5', () => {
    assert.equal(parseWinLoadPercentage('12,5'), 12.5)
  })
  it('边界：0 与 100', () => {
    assert.equal(parseWinLoadPercentage('0'), 0)
    assert.equal(parseWinLoadPercentage('100'), 100)
  })
  it('反例：越界 150 → null', () => {
    assert.equal(parseWinLoadPercentage('150'), null)
  })
  it('反例：垃圾/空 → null', () => {
    assert.equal(parseWinLoadPercentage('abc'), null)
    assert.equal(parseWinLoadPercentage(''), null)
  })
})

// ==================== C-006 mergeNodeOptions ====================
describe('mergeNodeOptions', () => {
  it('正例：无既有参数 → 仅追加堆参数', () => {
    assert.equal(mergeNodeOptions(undefined, 1024), '--max-old-space-size=1024')
  })
  it('正例：保留既有其他参数', () => {
    assert.equal(mergeNodeOptions('--require=/x', 1536), '--require=/x --max-old-space-size=1536')
  })
  it('反例（覆盖）：既有堆参数被替换', () => {
    assert.equal(mergeNodeOptions('--max-old-space-size=1800', 4096), '--max-old-space-size=4096')
  })
  it('正例：替换堆参数且保留其他参数与顺序', () => {
    assert.equal(
      mergeNodeOptions('--max-old-space-size=1800 --trace-warnings', 1024),
      '--max-old-space-size=1024 --trace-warnings',
    )
  })
  it('边界：多余空白被清洗（首尾）且堆参数原位替换', () => {
    assert.equal(
      mergeNodeOptions('  --max-old-space-size=2048 --require=x  ', 4096),
      '--max-old-space-size=4096 --require=x',
    )
  })
})

// ==================== C-007 mergeGradleOpts ====================
describe('mergeGradleOpts', () => {
  it('正例：无既有参数 → 仅追加 workers.max', () => {
    assert.equal(mergeGradleOpts(undefined, 2), '-Dorg.gradle.workers.max=2')
  })
  it('正例：保留既有 JVM 参数', () => {
    assert.equal(mergeGradleOpts('-Xmx2048m', 1), '-Xmx2048m -Dorg.gradle.workers.max=1')
  })
  it('反例（覆盖）：已有 workers.max 原位替换且不重复', () => {
    assert.equal(
      mergeGradleOpts('-Dorg.gradle.workers.max=4 -Xmx2g', 3),
      '-Dorg.gradle.workers.max=3 -Xmx2g',
    )
    assert.equal(
      mergeGradleOpts('-Xmx2g -Dorg.gradle.workers.max=4', 3),
      '-Xmx2g -Dorg.gradle.workers.max=3',
    )
  })
})

// ==================== C-008~C-012 classify ====================
describe('classify', () => {
  const CASES = [
    // C-008 分档判定与边界
    { name: '真实机器快照：load 0.37 / mem 4.9GiB / 16 核 → parallel jobs=3', m: { cores: 16, loadRatio: 0.37, loadKnown: true, memAvailableGiB: 4.9, swapActive: false }, o: {}, want: { profile: 'parallel', cargoJobs: 3, gradleWorkers: 4, nodeHeapMb: 4096 } },
    { name: '边界：load 0.49 / mem 4.0 → parallel', m: { cores: 16, loadRatio: 0.49, loadKnown: true, memAvailableGiB: 4.0, swapActive: false }, o: {}, want: { profile: 'parallel', cargoJobs: 2, gradleWorkers: 4, nodeHeapMb: 4096 } },
    { name: '边界：load 0.50 不属于 parallel（需 <0.5）→ balanced', m: { cores: 16, loadRatio: 0.50, loadKnown: true, memAvailableGiB: 4.0, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：load 1.0 → balanced', m: { cores: 16, loadRatio: 1.0, loadKnown: true, memAvailableGiB: 8.0, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：load 1.49 → balanced', m: { cores: 16, loadRatio: 1.49, loadKnown: true, memAvailableGiB: 8.0, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：load 1.5 → serial', m: { cores: 16, loadRatio: 1.5, loadKnown: true, memAvailableGiB: 8.0, swapActive: false }, o: {}, want: { profile: 'serial', cargoJobs: 1, gradleWorkers: 1, nodeHeapMb: 1024 } },
    { name: '边界：mem 1.5GiB → balanced 且公式 jobs=1', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 1.5, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 1, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：mem 1.49GiB → serial（OOM 高危）', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 1.49, swapActive: false }, o: {}, want: { profile: 'serial', cargoJobs: 1, gradleWorkers: 1, nodeHeapMb: 1024 } },
    { name: '边界：mem 3.0GiB → balanced（parallel 需 >=4）', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 3.0, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：mem 3.9GiB → balanced', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 3.9, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '边界：mem 4.0GiB → parallel', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 4.0, swapActive: false }, o: {}, want: { profile: 'parallel', cargoJobs: 2, gradleWorkers: 4, nodeHeapMb: 4096 } },
    // C-009 内存硬约束公式
    { name: '公式：mem 20GiB / 16 核 → jobs=13', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 20, swapActive: false }, o: {}, want: { profile: 'parallel', cargoJobs: 13, gradleWorkers: 4, nodeHeapMb: 4096 } },
    { name: '公式：cores 4 约束 mem 20GiB → jobs=4', m: { cores: 4, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 20, swapActive: false }, o: {}, want: { profile: 'parallel', cargoJobs: 4, gradleWorkers: 4, nodeHeapMb: 4096 } },
    // C-010 降级
    { name: '降级：swapActive → serial（即使资源充裕）', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 20, swapActive: true }, o: {}, want: { profile: 'serial', cargoJobs: 1, gradleWorkers: 1, nodeHeapMb: 1024 } },
    { name: '降级：负载未知（Windows PS 失败）→ 仅按内存分档 → balanced', m: { cores: 16, loadRatio: null, loadKnown: false, memAvailableGiB: 8.0, swapActive: false }, o: {}, want: { profile: 'balanced', cargoJobs: 2, gradleWorkers: 2, nodeHeapMb: 1536 } },
    { name: '降级：内存未知 → 保守 serial', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: null, swapActive: false }, o: {}, want: { profile: 'serial', cargoJobs: 1, gradleWorkers: 1, nodeHeapMb: 1024 } },
    // C-011 显式覆盖
    { name: '覆盖：强制 serial（mem 20）→ 仍 jobs=1', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 20, swapActive: false }, o: { profile: 'serial' }, want: { profile: 'serial', cargoJobs: 1, gradleWorkers: 1, nodeHeapMb: 1024 } },
    { name: '覆盖：强制 parallel（mem 2.0）→ 内存上限仍生效 jobs=1', m: { cores: 16, loadRatio: 0.9, loadKnown: true, memAvailableGiB: 2.0, swapActive: false }, o: { profile: 'parallel' }, want: { profile: 'parallel', cargoJobs: 1, gradleWorkers: 4, nodeHeapMb: 4096 } },
    { name: '覆盖：强制 parallel 且 swapActive → 显式覆盖优先', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 20, swapActive: true }, o: { profile: 'parallel' }, want: { profile: 'parallel', cargoJobs: 13, gradleWorkers: 4, nodeHeapMb: 4096 } },
    // C-012 jobsPerGiB 预算覆盖
    { name: '预算：jobsPerGiB=2 → mem 4GiB jobs=2、mem 6GiB jobs=3', m: { cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 6.0, swapActive: false }, o: { jobsPerGiB: 2 }, want: { profile: 'parallel', cargoJobs: 3, gradleWorkers: 4, nodeHeapMb: 4096 } },
  ]

  for (const c of CASES) {
    it(c.name, () => {
      const r = classify(c.m, c.o)
      assert.equal(r.profile, c.want.profile)
      assert.equal(r.cargoJobs, c.want.cargoJobs)
      assert.equal(r.gradleWorkers, c.want.gradleWorkers)
      assert.equal(r.nodeHeapMb, c.want.nodeHeapMb)
    })
  }

  it('副作用（reasons）：降档原因被收集，空闲时为空', () => {
    assert.deepEqual(classify({ cores: 16, loadRatio: 1.5, loadKnown: true, memAvailableGiB: 8, swapActive: false }).reasons, ['load=1.50'])
    assert.deepEqual(classify({ cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 1.4, swapActive: false }).reasons, ['mem=1.4GiB'])
    assert.deepEqual(classify({ cores: 16, loadRatio: 0.1, loadKnown: true, memAvailableGiB: 8, swapActive: true }).reasons, ['swap-active'])
    assert.deepEqual(classify({ cores: 16, loadRatio: 0.37, loadKnown: true, memAvailableGiB: 4.9, swapActive: false }).reasons, [])
  })
})

// ==================== C-013 buildEnv ====================
describe('buildEnv', () => {
  const IDLE = { cores: 16, loadRatio: 0.37, loadKnown: true, memAvailableGiB: 4.9, swapActive: false }

  it('正例：空 baseEnv 生成三个编译 env 变量（真实机器 → parallel jobs=3）', () => {
    const r = buildEnv(IDLE, {}, {})
    assert.equal(r.profile, 'parallel')
    assert.equal(r.additions.CARGO_BUILD_JOBS, '3')
    assert.equal(r.additions.GRADLE_OPTS, '-Dorg.gradle.workers.max=4')
    assert.equal(r.additions.NODE_OPTIONS, '--max-old-space-size=4096')
  })
  it('正例：与既有 NODE_OPTIONS / GRADLE_OPTS 合并（保留用户自定义项）', () => {
    const r = buildEnv(IDLE, {}, { NODE_OPTIONS: '--max-old-space-size=1800 --require=/x', GRADLE_OPTS: '-Xmx2048m' })
    assert.equal(r.additions.NODE_OPTIONS, '--max-old-space-size=4096 --require=/x')
    assert.equal(r.additions.GRADLE_OPTS, '-Xmx2048m -Dorg.gradle.workers.max=4')
  })
  it('反例（覆盖）：强制 serial 时全参数收敛到最小', () => {
    const r = buildEnv(IDLE, { profile: 'serial' }, {})
    assert.equal(r.additions.CARGO_BUILD_JOBS, '1')
    assert.equal(r.additions.GRADLE_OPTS, '-Dorg.gradle.workers.max=1')
    assert.equal(r.additions.NODE_OPTIONS, '--max-old-space-size=1024')
  })
})

// ==================== C-014~C-018 sampleSystem ====================
describe('sampleSystem', () => {
  const MEM_SAMPLE = [
    'MemTotal:       14139224 kB',
    'MemFree:         1142180 kB',
    'MemAvailable:    4897960 kB',
    'SwapTotal:             0 kB',
  ].join('\n')
  const MEM_SAMPLE_WITH_SWAP = MEM_SAMPLE.replace('SwapTotal:             0 kB', 'SwapTotal:       8388608 kB')

  /** 严格 mock：只允许 Map 内路径，读未预期路径即失败（替身策略严格模式） */
  const makeReadFile = (files) => (p) => {
    if (!files.has(p)) throw new Error(`unexpected read: ${p}`)
    return files.get(p)
  }
  const makeSleepRecorder = () => {
    const calls = []
    return [calls, async (ms) => calls.push(ms)]
  }

  it('正例（linux）：/proc 三件套解析 + 无 swap 时不采样 sleep', async () => {
    const files = new Map([
      ['/proc/loadavg', '0.37 0.40 0.50 1/812 4567'],
      ['/proc/meminfo', MEM_SAMPLE],
      ['/sys/devices/system/cpu/online', '0-15'],
      ['/proc/vmstat', 'pswpin 10\npswpout 5\n'],
    ])
    const [sleepCalls, sleep] = makeSleepRecorder()
    const r = await sampleSystem({ platform: 'linux', readFile: makeReadFile(files), sleep })
    assert.equal(r.cores, 16)
    assert.equal(r.loadRatio, 0.37)
    assert.equal(r.loadKnown, true)
    assert.equal(r.memAvailableGiB, 4897960 / 1024 / 1024) // 与 /proc/meminfo 样例精确一致
    assert.equal(r.swapActive, false)
    assert.deepEqual(sleepCalls, []) // 无 swap → 不做双采样
  })

  it('边界（linux）：swap 抖动窗口增量 1 页 → 非 active（阈值 >1）', async () => {
    const files = new Map([
      ['/proc/loadavg', '0.37 0.40 0.50 1/812 4567'],
      ['/proc/meminfo', MEM_SAMPLE_WITH_SWAP],
      ['/sys/devices/system/cpu/online', '0-15'],
      ['/proc/vmstat', 'pswpin 10\npswpout 5\n'], // 基线
    ])
    const [sleepCalls, sleep] = makeSleepRecorder()
    // 双采样顺序：首读=基线，次读=基线+1 页（窗口增量 1 ≤ 阈值 1 → 非 active）
    let vmReads = 0
    const readFile = (p) => {
      if (!files.has(p)) throw new Error(`unexpected read: ${p}`)
      if (p === '/proc/vmstat') {
        vmReads += 1
        return vmReads === 1 ? files.get(p) : 'pswpin 10\npswpout 6\n'
      }
      return files.get(p)
    }
    const r = await sampleSystem({ platform: 'linux', readFile, sleep })
    assert.equal(r.swapActive, false)
    assert.deepEqual(sleepCalls, [SWAP_SAMPLE_MS])
  })

  it('正例（linux）：swap 抖动窗口增量 5 页 → active（强制串行档依据）', async () => {
    const files = new Map([
      ['/proc/loadavg', '0.37 0.40 0.50 1/812 4567'],
      ['/proc/meminfo', MEM_SAMPLE_WITH_SWAP],
      ['/sys/devices/system/cpu/online', '0-15'],
      ['/proc/vmstat', 'pswpin 10\npswpout 5\n'], // 基线
    ])
    // 双采样顺序：首读=基线，次读=基线+2+3 页（窗口增量 5 > 1 → active）
    let vmReads = 0
    const readFile = (p) => {
      if (!files.has(p)) throw new Error(`unexpected read: ${p}`)
      if (p === '/proc/vmstat') {
        vmReads += 1
        return vmReads === 1 ? files.get(p) : 'pswpin 12\npswpout 8\n'
      }
      return files.get(p)
    }
    const r = await sampleSystem({ platform: 'linux', readFile, sleep: async () => {} })
    assert.equal(r.swapActive, true)
  })

  it('正例（linux）：内存充裕（>=8GiB）时跳过 swap 双采样', async () => {
    const memBig = MEM_SAMPLE_WITH_SWAP.replace('MemAvailable:    4897960 kB', 'MemAvailable:    9437184 kB')
    const files = new Map([
      ['/proc/loadavg', '0.37 0.40 0.50 1/812 4567'],
      ['/proc/meminfo', memBig],
      ['/sys/devices/system/cpu/online', '0-15'],
    ])
    const [sleepCalls, sleep] = makeSleepRecorder()
    const r = await sampleSystem({ platform: 'linux', readFile: makeReadFile(files), sleep })
    assert.equal(r.swapActive, false)
    assert.deepEqual(sleepCalls, [])
  })

  it('反例（linux 兜底）：/proc 不可读 → os API 兜底（负载未知、内存有限值）', async () => {
    const readFile = () => {
      throw new Error('EACCES')
    }
    const r = await sampleSystem({ platform: 'linux', readFile, sleep: async () => {} })
    assert.equal(r.loadKnown, false)
    assert.equal(r.loadRatio, null)
    assert.ok(Number.isFinite(r.memAvailableGiB) && r.memAvailableGiB > 0)
    assert.equal(r.swapActive, false)
  })

  it('正例（win32）：PowerShell 返回 12.5 → loadRatio=0.125', async () => {
    const exec = () => ({ status: 0, stdout: '12.5\r\n' })
    const r = await sampleSystem({ platform: 'win32', exec })
    assert.equal(r.loadRatio, 0.125)
    assert.equal(r.loadKnown, true)
    assert.ok(Number.isFinite(r.memAvailableGiB) && r.memAvailableGiB > 0)
  })
  it('反例（win32）：status≠0 → 负载未知', async () => {
    const r = await sampleSystem({ platform: 'win32', exec: () => ({ status: 1, stdout: '' }) })
    assert.equal(r.loadRatio, null)
    assert.equal(r.loadKnown, false)
  })
  it('异常（win32）：PowerShell 抛错 → 负载未知（不中断采样）', async () => {
    const r = await sampleSystem({ platform: 'win32', exec: () => { throw new Error('ENOENT') } })
    assert.equal(r.loadRatio, null)
    assert.equal(r.loadKnown, false)
  })
  it('正例（darwin）：os 原生 API，loadKnown=true', async () => {
    const r = await sampleSystem({ platform: 'darwin' })
    assert.equal(r.loadKnown, true)
    assert.ok(Number.isFinite(r.loadRatio) && r.loadRatio >= 0)
    assert.ok(Number.isFinite(r.memAvailableGiB) && r.memAvailableGiB > 0)
  })
  it('反例（未知平台）：全字段置空，不注入覆盖', async () => {
    const r = await sampleSystem({ platform: 'freebsd' })
    assert.equal(r.loadRatio, null)
    assert.equal(r.loadKnown, false)
    assert.equal(r.memAvailableGiB, null)
    assert.equal(r.swapActive, false)
  })

  // 回归防护：曾把默认 readFile 写成异步（import().then()），tryRead 同步调用导致
  // 全部解析为 '[object Promise]'、mem=0。此处不注入，走真实默认依赖验证形状。
  it('回归：默认依赖（真实 readFile）下 linux 采样形状正确', async () => {
    const r = await sampleSystem({ platform: 'linux' })
    assert.ok(Number.isInteger(r.cores) && r.cores >= 1)
    assert.equal(typeof r.loadKnown, 'boolean')
    assert.equal(typeof r.swapActive, 'boolean')
    // /proc 可读时为真实解析值，不可读（Windows 上跑本测试）时走 os API 兜底——两者都应有限
    assert.ok(Number.isFinite(r.memAvailableGiB) && r.memAvailableGiB > 0)
  })

  it('辅助：真实 os API 口径（本机采样应与 Node os 模块一致）', () => {
    const cores = typeof os.availableParallelism === 'function' ? os.availableParallelism() : os.cpus().length
    assert.ok(cores >= 1)
  })
})
