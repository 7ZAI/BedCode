/**
 * TUI 滚动兼容（移动端）
 *
 * 解决 alt-screen TUI 应用（opencode 等）在移动端无法滚动的问题：
 * 备用屏幕缓冲区无 scrollback，移动端 xterm 的 scrollToLine 无内容可滚；
 * 应用内部滚动又需要终端把滚轮意图以 SGR 鼠标序列送达。
 *
 * 机制（ADR-0013）：
 * - 双条件门控：xterm 处于备用屏幕（buffer.active.type === 'alternate'）
 *   且应用启用了 SGR 鼠标上报（输出流嗅探 DECSET 1006h）才视为 TUI 模式
 * - TUI 模式下触摸拖动/惯性翻译成 SGR 滚轮序列（ESC[<64/65;col;rowM），
 *   经既有 WS 通道（ws_send_input_async）原样写入主机 PTY，由应用自行滚动
 * - 节流 ~40ms 合并 + 发送在途（inflight）时丢弃积压：最新滚动意图优先，
 *   避免给应用灌入延迟拖尾的滚轮事件；与自研 WS client 的背压语义互为防线
 * - 退出备用屏幕（应用退出/会话停止）自动恢复现有 scrollback 滚动
 */

import { ref, type Ref } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { wsSendInput } from '@/composables/useMobileCommands'

// ==================== 常量 ====================

/** SGR 滚轮按钮号：64=上滚（wheel up），65=下滚（wheel down） */
const WHEEL_UP_BUTTON = 64
const WHEEL_DOWN_BUTTON = 65

/** 滚轮事件节流窗口（毫秒）：窗口内多次拖动合并为一个发送 */
const WHEEL_THROTTLE_MS = 40

/** 单次发送的滚轮事件上限：超出的累积丢弃（持续拖动会继续累积补偿） */
const MAX_WHEEL_EVENTS_PER_SEND = 10

/** 输出流嗅探尾部保留长度：`ESC[?1006l` 最长 9 字符，留足跨 chunk 切分余量 */
const TAIL_KEEP_CHARS = 16

/** DECSET 模式序列匹配：ESC [ ? <数字> <h|l> */
const DECSET_QUESTION_RE = /\x1b\[\?(\d+)([hl])/g

// ==================== 纯函数（可单测） ====================

/**
 * SGR 鼠标上报嗅探器：跟踪输出流中应用启用的 DECSET 1006（SGR 坐标格式）。
 *
 * 输出被 WS/合并管线切成任意 chunk，CSI 序列可能跨 chunk——内部保留尾部
 * 片段（TAIL_KEEP_CHARS），下一 chunk 到达时拼接后重新扫描。
 * 1006 关闭序列（1006l）一并跟踪，退出时复位。
 */
export interface MouseSgrSniffer {
  /** 应用当前是否启用了 SGR 鼠标上报 */
  readonly enabled: boolean
  /** 喂入一段输出字节（写入 xterm 前调用） */
  feed(data: Uint8Array): void
  /** 复位（会话停止/断开时调用） */
  reset(): void
}

export function createMouseSgrSniffer(): MouseSgrSniffer {
  let tail = ''
  let mouseSgrEnabled = false
  const decoder = new TextDecoder()

  return {
    get enabled() {
      return mouseSgrEnabled
    },
    feed(data: Uint8Array) {
      const text = tail + decoder.decode(data)
      for (const m of text.matchAll(DECSET_QUESTION_RE)) {
        const mode = Number(m[1])
        const on = m[2] === 'h'
        if (mode === 1006) {
          mouseSgrEnabled = on
        }
      }
      tail = text.slice(-TAIL_KEEP_CHARS)
    },
    reset() {
      tail = ''
      mouseSgrEnabled = false
    },
  }
}

/**
 * 生成 SGR 滚轮序列：`ESC[<64/65;col;rowM`。
 *
 * deltaLines > 0 表示向下查看（手指上滑，等价于滚轮下滚 button 65）；
 * deltaLines < 0 向上查看（button 64）。col/row 为 1-based 终端格坐标。
 * 单次最多生成 MAX_WHEEL_EVENTS_PER_SEND 个事件，超出部分丢弃。
 */
export function createSgrWheelSequence(
  deltaLines: number,
  col: number,
  row: number,
): string {
  if (deltaLines === 0) return ''
  const button = deltaLines > 0 ? WHEEL_DOWN_BUTTON : WHEEL_UP_BUTTON
  const count = Math.min(Math.abs(deltaLines), MAX_WHEEL_EVENTS_PER_SEND)
  let seq = ''
  for (let i = 0; i < count; i++) {
    seq += `\x1b[<${button};${col};${row}M`
  }
  return seq
}

// ==================== Composable ====================

export function useTuiCompat(sessionId: string) {
  /** TUI 模式：备用屏幕 + 应用启用 SGR 鼠标上报，双条件门控 */
  const isTuiMode: Ref<boolean> = ref(false)

  let terminal: Terminal | null = null
  let writeParsedDisposable: { dispose(): void } | null = null
  let altScreen = false
  const sniffer = createMouseSgrSniffer()

  // 节流状态
  let pendingDelta = 0
  let pendingCol = 1
  let pendingRow = 1
  let throttleTimer: ReturnType<typeof setTimeout> | null = null
  /** 上一次 WS 发送在途：在途期间到期窗口直接丢弃（最新目标优先） */
  let inflight = false

  function updateMode() {
    isTuiMode.value = altScreen && sniffer.enabled
  }

  /**
   * 挂接终端：onWriteParsed（写解析完成）后检查当前缓冲区类型。
   * 由 xterm 解析器维护，无转义切分问题；回放（forceReplay）期间同样正确
   */
  function attach(term: Terminal) {
    terminal = term
    writeParsedDisposable = term.onWriteParsed(() => {
      altScreen = term.buffer.active.type === 'alternate'
      updateMode()
    })
  }

  /**
   * 喂入输出字节（写入 xterm 前调用，registerRealtimeHandler 的 onRawOutput 钩子）。
   * 在合并前的原始 chunk 上嗅探，chunk 边界即网络/事件边界
   */
  function feedOutput(data: Uint8Array) {
    sniffer.feed(data)
    updateMode()
  }

  /**
   * 发送滚轮事件（TUI 模式）：累积 delta 并节流合并，
   * 窗口到期生成 SGR 序列经 WS 送达 PTY，fire-and-forget
   */
  function sendWheel(deltaLines: number, col: number, row: number) {
    if (!isTuiMode.value || deltaLines === 0) return

    pendingDelta += deltaLines
    pendingCol = col
    pendingRow = row

    if (throttleTimer) return
    throttleTimer = setTimeout(() => {
      throttleTimer = null
      const delta = pendingDelta
      pendingDelta = 0
      if (delta === 0 || inflight) return

      const seq = createSgrWheelSequence(delta, pendingCol, pendingRow)
      if (!seq) return

      inflight = true
      wsSendInput(sessionId, seq)
        .catch(() => {
          // 发送失败静默降级：下个窗口/下一次手势自然重试
        })
        .finally(() => {
          inflight = false
        })
    }, WHEEL_THROTTLE_MS)
  }

  function dispose() {
    if (throttleTimer) {
      clearTimeout(throttleTimer)
      throttleTimer = null
    }
    writeParsedDisposable?.dispose()
    writeParsedDisposable = null
    terminal = null
    sniffer.reset()
    altScreen = false
    pendingDelta = 0
    isTuiMode.value = false
  }

  return {
    isTuiMode,
    attach,
    feedOutput,
    sendWheel,
    dispose,
  }
}
