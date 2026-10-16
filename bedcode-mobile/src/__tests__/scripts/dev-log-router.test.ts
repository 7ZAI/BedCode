/**
 * dev-log 行路由与双写通路的行为契约（scripts/lib/dev-log-router.js）
 *
 * 背景：`pnpm run tauri:android:dev:log` 控制台输出曾无颜色。根因有两层——
 *   ① 脚本把子进程 stdout 接到管道（要过滤 + 落盘），tauri CLI / cargo / Gradle
 *      判定非 TTY 就不产 ANSI；
 *   ② 更直接：剥 ANSI 的 stripAnsi 被套在**进入行缓冲之前**，控制台与落盘双写
 *      同一份已剥色文本（bb52d90f3 起，文件头注释还写着「控制台保留彩色」，与事实不符）。
 * 本模块把「控制台原样透传 / 落盘纯文本 / 过滤判定跑剥色文本」三件事拆开：
 * 每个完整行产出 raw（原文，给控制台）与 clean（纯文本，给判定与落盘）两份形态。
 *
 * 契约表：
 *   C-01 控制台保留 ANSI：raw 形态含原样的 CSI / OSC 序列，不被剥色
 *   C-02 落盘纯文本：clean 形态无任何 ANSI 序列、无 \r（grep / cat 友好）
 *   C-03 过滤判定基准：clean 已剥色，带颜色的行仍能被行内容正则正确判定
 *      （行首锚定规则尤其：跑 raw 会因前导转义码失配、噪声漏放）
 *   C-04 行缓冲：chunk 尾的半行不成对返回，等下一批补齐 \n 才返回
 *   C-05 退出冲刷：flush 取走残留半行，且同一行不会被取走两次
 *   C-06 行对不变量：stripAnsi 不增删换行（OSC 一律不跨行，宁可残留不吞换行），行数恒一一对应
 *   C-07 空输入：空 chunk 不产出行对；空 clean + 有控制序列的行由调用方原样透传
 *   C-08 未知 fd 抛错（接线错误必须立刻暴露，不能静默丢整条日志）
 *   C-09 跨 chunk 的半截 ANSI 序列不误删正文（剥色不跨边界，宁可残留不吞内容）
 */
import { describe, expect, it } from 'vitest'

import { createLineRouter, stripAnsi } from '../../../scripts/lib/dev-log-router.js'

/**
 * 与 android-dev-log.js 的过滤判定同形：多数丢弃规则是行首锚定的
 * （/^> Task :/、LOGCAT_LINE 等）——带颜色时若跑在 raw 上会全部失配、噪声漏放
 */
const NOISE_PATTERN = /cranelift_codegen::/
const ANCHORED_NOISE_PATTERN = /^> Task :app:assemble/

/** 收集一整条输出通路的双写结果（模拟脚本 flushLine 的两个写点） */
function drain(router: ReturnType<typeof createLineRouter>, fd: string, ...chunks: string[]) {
  const consoleLines: string[] = []
  const fileLines: string[] = []
  for (const chunk of chunks) {
    for (const row of router.push(fd, chunk)) {
      consoleLines.push(row.raw) // 控制台：原样透传
      if (row.clean !== '') fileLines.push(row.clean) // 落盘：纯文本
    }
  }
  return { consoleLines, fileLines }
}

describe('stripAnsi', () => {
  it('剥掉 CSI 颜色与清屏序列_当输入含 ANSI', () => {
    expect(stripAnsi('\x1b[32mINFO\x1b[0m ready')).toBe('INFO ready')
    expect(stripAnsi('\x1b[2J\x1b[?25l done')).toBe(' done')
  })

  it('剥掉 OSC 终端标题_当输入含 BEL 终止的 OSC', () => {
    expect(stripAnsi('\x1b]0;bedcode\x07start')).toBe('start')
  })

  it('剥掉 OSC 的 ST 终止形式_当序列以 ESC 反斜杠结尾', () => {
    expect(stripAnsi('\x1b]0;bedcode\x1b\\start')).toBe('start')
  })

  it('剥掉回车_当输入用 CR 覆盖同一行', () => {
    expect(stripAnsi('50%\r100%')).toBe('50%100%')
  })

  it('原样返回无 ANSI 的文本_当输入本身干净', () => {
    expect(stripAnsi('bedcode_mobile_lib::pty::spawn')).toBe('bedcode_mobile_lib::pty::spawn')
  })

  it('不吞掉 OSC 里的换行_当标题串异常含裸换行', () => {
    // 行为取舍：OSC 字符类排除 \x0a 后，这条畸形序列**整体不匹配**、原样残留，
    // 但换行一定保留 —— clean 与 raw 的行数因此恒等，双写不错位（C-06）。
    // 反例防线：若 OSC 贪婪跨行（字符类含 \x0a），换行被吃掉，clean 少一行 → 双写串行
    expect(stripAnsi('\x1b]0;ti\ntle\x07rest')).toBe('\x1b]0;ti\ntle\x07rest')
  })

  it('不删正文_当序列在文本中间被截断（跨 chunk 半截）', () => {
    // 反例防线：剥色不跨 chunk 边界，半截序列宁可残留也不误删后文
    expect(stripAnsi('before\x1b[38')).toBe('before\x1b[38')
  })
})

describe('createLineRouter 双写形态', () => {
  it('控制台行保留 ANSI 颜色_当子进程吐出彩色日志', () => {
    const router = createLineRouter(['stdout'])
    const { consoleLines } = drain(router, 'stdout', '\x1b[32mINFO\x1b[0m ready\n')

    expect(consoleLines).toEqual(['\x1b[32mINFO\x1b[0m ready'])
  })

  it('落盘行不含任何 ANSI 与回车_当同一行既有颜色又有进度条回车', () => {
    const router = createLineRouter(['stdout'])
    const { fileLines } = drain(router, 'stdout', '\x1b[32m50%\x1b[0m\rdone\n')

    expect(fileLines).toEqual(['50%done'])
    expect(fileLines[0]).not.toMatch(/\x1b|\r/)
  })

  it('控制台保留回车_当子进程用 CR 覆盖刷新同一行', () => {
    const router = createLineRouter(['stdout'])
    const { consoleLines } = drain(router, 'stdout', 'downloading 50%\rdownloading 100%\n')

    expect(consoleLines).toEqual(['downloading 50%\rdownloading 100%'])
  })

  it('带颜色的噪音行仍能被内容正则判定为丢弃_当判定跑在 clean 形态', () => {
    const router = createLineRouter(['stdout'])
    const rows = router.push('stdout', '\x1b[2mcranelift_codegen::x86 compile\x1b[0m\n')

    expect(rows).toHaveLength(1)
    expect(NOISE_PATTERN.test(rows[0].clean)).toBe(true)
  })

  it('行首锚定的丢弃规则在 clean 上命中、在 raw 上失配_因此判定必须用 clean', () => {
    const router = createLineRouter(['stdout'])
    const rows = router.push('stdout', '\x1b[1m> Task :app:assembleDebug\x1b[0m\n')

    expect(ANCHORED_NOISE_PATTERN.test(rows[0].clean)).toBe(true)
    // 反例：跑 raw 会漏放 → Gradle 进展噪声重新灌进控制台与日志文件
    expect(ANCHORED_NOISE_PATTERN.test(rows[0].raw)).toBe(false)
  })
})

describe('createLineRouter 行缓冲边界', () => {
  it('半行不成对返回_当 chunk 以换行前的内容结尾', () => {
    const router = createLineRouter(['stdout'])
    expect(router.push('stdout', 'INFO rea')).toEqual([])
  })

  it('跨 chunk 拼回完整行后返回且内容正确_当换行落在下一批', () => {
    const router = createLineRouter(['stdout'])
    router.push('stdout', '\x1b[32mINFO rea')
    const rows = router.push('stdout', 'dy\x1b[0m\n')

    expect(rows).toEqual([{ raw: '\x1b[32mINFO ready\x1b[0m', clean: 'INFO ready' }])
  })

  it('一批多行一次全部返回_当 chunk 内含多个换行', () => {
    const router = createLineRouter(['stdout'])
    const rows = router.push('stdout', '\x1b[32ma\x1b[0m\nb\n\r\n')

    expect(rows.map((r) => r.clean)).toEqual(['a', 'b', ''])
    expect(rows.map((r) => r.raw)).toEqual(['\x1b[32ma\x1b[0m', 'b', '\r'])
  })

  it('行对数量恒等_当 OSC 序列异常含裸换行', () => {
    // 反例防线：clean 多出一行而 raw 不出（或反之）→ 双写错位
    const router = createLineRouter(['stdout'])
    const rows = router.push('stdout', '\x1b]0;ti\ntle\x07first\nsecond\n')

    expect(rows.map((r) => r.clean)).toEqual(['\x1b]0;ti', 'tle\x07first', 'second'])
    expect(rows.map((r) => r.raw)).toEqual(['\x1b]0;ti', 'tle\x07first', 'second'])
  })

  it('两路通道各自独立缓冲_当 stdout 与 stderr 交错到达', () => {
    const router = createLineRouter(['stdout', 'stderr'])
    router.push('stdout', 'out-part')
    router.push('stderr', '\x1b[31merr-part\x1b[0m') // 未换行，退出时才成对

    const outRows = router.push('stdout', '-tail\n')
    const errTail = router.flush('stderr')

    expect(outRows).toEqual([{ raw: 'out-part-tail', clean: 'out-part-tail' }])
    expect(errTail).toEqual({ raw: '\x1b[31merr-part\x1b[0m', clean: 'err-part' })
  })

  it('空输入不产出行对_当 chunk 为空串', () => {
    const router = createLineRouter(['stdout'])

    expect(router.push('stdout', '')).toEqual([])
    expect(router.flush('stdout')).toBeNull()
  })
})

describe('createLineRouter 退出冲刷', () => {
  it('取走残留半行_当进程退出时缓冲仍有未换行内容', () => {
    const router = createLineRouter(['stdout'])
    router.push('stdout', '\x1b[32mINFO no newline\x1b[0m')

    expect(router.flush('stdout')).toEqual({
      raw: '\x1b[32mINFO no newline\x1b[0m',
      clean: 'INFO no newline',
    })
  })

  it('同一行不会被取走两次_当连续 flush 同一通道', () => {
    const router = createLineRouter(['stdout'])
    router.push('stdout', 'tail')

    expect(router.flush('stdout')).not.toBeNull()
    expect(router.flush('stdout')).toBeNull()
  })

  it('空行通道 flush 返回 null_当该通道从未收到数据', () => {
    const router = createLineRouter(['stdout', 'stderr'])

    expect(router.flush('stderr')).toBeNull()
  })
})

describe('createLineRouter 接线防御', () => {
  it('未知 fd 抛错_当 push 传入未注册通道', () => {
    const router = createLineRouter(['stdout'])

    expect(() => router.push('stdin', 'x')).toThrow(/未知 fd/)
  })

  it('未知 fd 抛错_当 flush 传入未注册通道', () => {
    const router = createLineRouter(['stdout'])

    expect(() => router.flush('stdin')).toThrow(/未知 fd/)
  })
})