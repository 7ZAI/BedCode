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
 * - 节流 ~16ms 合并发送；发送在途（inflight）时不丢弃积压——滚动量保留，
 *   窗口结束后补发（否则快速翻历史时滚动距离严重缩水，体感只能滚一屏多）；
 *   仅积压超限（一屏两倍）时丢弃最旧部分；每窗口至多发送 2 行（超出部分
 *   随下一窗口补发），把单帧批量跳跃摊平成逐窗口小步推进，减少顿感；
 *   与自研 WS client 的背压语义互为防线
 * - 退出备用屏幕（应用退出/会话停止）自动恢复现有 scrollback 滚动
 */

import { ref, type Ref } from 'vue'
import type { Terminal } from '@xterm/xterm'
import { wsSendInput } from '@/composables/useMobileCommands'

// ==================== 常量 ====================

/** SGR 滚轮按钮号：64=上滚（wheel up），65=下滚（wheel down） */
const WHEEL_UP_BUTTON = 64
const WHEEL_DOWN_BUTTON = 65

/** 滚轮事件节流窗口（毫秒）：窗口内多次拖动合并为一个发送。
 * 对齐一帧（16ms）：窗口越短事件流越连续，应用滚动越跟手；
 * 窗口内累积行数由 MAX_WHEEL_EVENTS_PER_WINDOW 限制 */
const WHEEL_THROTTLE_MS = 16

/**
 * 单次序列生成的事件上限：createSgrWheelSequence 的生成安全网
 * （窗口发送量已由 MAX_WHEEL_EVENTS_PER_WINDOW 控制，此上限仅防极端值）
 */
const MAX_WHEEL_EVENTS_PER_SEND = 60

/**
 * 单次发送窗口的滚轮事件上限：窗口内累积超过该值时只发一部分，剩余积压
 * 随下一窗口补发。把一次手势的批量跳跃（单帧多行）摊平成每窗口 1~2 行，
 * 应用侧逐窗口重绘，快速滑动时不再整段跳变（减少顿感）；
 * 每窗口 2 行 ≈ 125 行/秒，足够覆盖拖动与甩动速度
 */
const MAX_WHEEL_EVENTS_PER_WINDOW = 2

/**
 * 积压丢弃上限（行）：发送在途期间新累积的滚动量超过该上限时，
 * 只保留最近部分（最新滚动意图优先），防止网络持续拥塞时无限堆积
 */
const MAX_PENDING_DELTA = 120

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

  /** 调度发送窗口：发送在途时不丢积压，由下一个窗口补发 */
  function scheduleSend() {
    if (throttleTimer) return
    throttleTimer = setTimeout(() => {
      throttleTimer = null
      const delta = pendingDelta
      if (delta === 0) return
      if (inflight) {
        // 上次发送仍在途：保留积压不清零（丢弃会丢失大量滚动量，
        // 体感「只能滚一屏多一点」），窗口到期后由下一次调度补发
        scheduleSend()
        return
      }
      // 每窗口至多发送 MAX_WHEEL_EVENTS_PER_WINDOW 行：超出部分留在积压
      // 随下一窗口补发（不丢弃），摊平单帧批量跳跃
      const capped = Math.max(-MAX_WHEEL_EVENTS_PER_WINDOW, Math.min(delta, MAX_WHEEL_EVENTS_PER_WINDOW))
      pendingDelta -= capped
      const seq = createSgrWheelSequence(capped, pendingCol, pendingRow)
      if (!seq) return

      inflight = true
      wsSendInput(sessionId, seq)
        .catch(() => {
          // 发送失败静默降级：下个窗口/下一次手势自然重试
        })
        .finally(() => {
          inflight = false
          // 在途期间可能已累积新积压：立即调度补发（不等下一次手势）
          if (pendingDelta !== 0) scheduleSend()
        })
    }, WHEEL_THROTTLE_MS)
  }

  /**
   * 发送滚轮事件（TUI 模式）：累积 delta 并节流合并，
   * 窗口到期生成 SGR 序列经 WS 送达 PTY，fire-and-forget
   */
  function sendWheel(deltaLines: number, col: number, row: number) {
    if (!isTuiMode.value || deltaLines === 0) return

    // 积压上限：超过上限丢弃最旧部分（保留最新滚动意图），防长期拥塞无限堆积
    pendingDelta = Math.max(-MAX_PENDING_DELTA, Math.min(pendingDelta + deltaLines, MAX_PENDING_DELTA))
    pendingCol = col
    pendingRow = row

    scheduleSend()
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
