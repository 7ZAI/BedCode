/**
 * TUI 滚动兼容（移动端）
 *
 * 解决 alt-screen TUI 应用（opencode 等）在移动端无法滚动的问题：
 * 备用屏幕缓冲区无 scrollback，移动端 xterm 的 scrollToLine 无内容可滚；
 * 应用内部滚动又需要终端把滚轮意图以 SGR 鼠标序列送达。
 *
 * 机制（ADR-0013）：
 * - 双条件门控：xterm 处于备用屏幕（buffer.active.type === 'alternate'）
 *   且应用启用了 SGR 鼠标上报（输出流嗅探 DECSET 1006 + 上报模式
 *   9/1000/1001/1002/1003，支持多参数合并序列 `ESC[?1000;1006h`）才视为 TUI 模式
 * - TUI 模式下触摸拖动/惯性翻译成 SGR 滚轮序列（ESC[<64/65;col;rowM），
 *   经既有 WS 通道（ws_send_input_async）原样写入主机 PTY，由应用自行滚动
 * - 灵敏度折算：主流 TUI 把单个滚轮事件放大为 ~3 行滚动（终端惯例），
 *   手势行数按 WHEEL_SENSITIVITY_NUM/DEN 折算后再入队，否则 TUI 内滚动比
 *   非 TUI 的 1:1 行滚动快约 3 倍；缩放余量保留在积压中，慢速拖动不丢意图
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

/**
 * 滚轮灵敏度（整数分数 NUM/DEN = 1/3）：手势行数 → SGR 滚轮事件数折算。
 *
 * 主流 TUI 应用（bubbletea viewport、opencode 等）把单个滚轮事件放大为
 * ~3 行滚动（终端惯例），1:1 直发会让 TUI 内滚动比非 TUI 的 1:1 行滚动
 * 快约 3 倍。折算后 3 行手指位移产生 1 个滚轮事件，应用侧放大 3 行 →
 * 体感与非 TUI 一致。
 *
 * 用整数分子/分母而非浮点系数：积压按分母缩放为整数累积（events×DEN），
 * 全程整数运算无精度损失——浮点 1/3 会让 3*(1/3)=0.99…9 被 trunc 丢弃，
 * 慢速拖动的滚动意图莫名消失。小数余量以缩放整数形式保留在积压中，
 * 累积到完整事件再发。若个别应用单事件只滚 1 行导致偏慢，可调大 NUM（≤DEN）。
 */
const WHEEL_SENSITIVITY_NUM = 1
const WHEEL_SENSITIVITY_DEN = 3

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

/** 输出流嗅探尾部保留长度：`ESC[?1000;1006l` 最长 14 字符，留足跨 chunk 切分余量 */
const TAIL_KEEP_CHARS = 20

/**
 * DECSET 模式序列匹配：ESC [ ? <数字>[;<数字>...] <h|l>
 *
 * 必须支持**多参数合并**写法（`ESC[?1000;1006h`）：部分 TUI 框架把多个模式
 * 合并进一条序列，只匹配单参数会整体漏判 → isTuiMode 恒 false → 手势走 xterm
 * 滚动，而备用屏幕没有 scrollback → 完全滚不动（真机症状：TUI 里滑不动）。
 */
const DECSET_QUESTION_RE = /\x1b\[\?([0-9;]+)([hl])/g

/**
 * 鼠标上报模式集合（X10=9 / 普通=1000 / 高亮=1001 / 按钮事件=1002 / 任意事件=1003）：
 * 滚轮事件只有这些模式开启时才会被应用接收。
 */
const MOUSE_TRACKING_MODES = [9, 1000, 1001, 1002, 1003]

/** SGR 坐标编码模式（1006）：决定滚轮序列的字节形态，非滚轮上报的使能开关 */
const MOUSE_SGR_MODE = 1006

// ==================== 纯函数（可单测） ====================

/**
 * SGR 鼠标上报嗅探器：跟踪输出流中应用启用的 DECSET 模式——SGR 编码（1006）
 * 与鼠标上报（9/1000/1001/1002/1003）。
 *
 * 双条件缺一不可：只置 1006 时应用并不接收滚轮事件（把滚轮序列发过去无人处理，
 * 表现为「模拟 TUI 模式下滚不动」）；只置上报模式时序列字节形态又不是 SGR。
 * 两者同时成立才算真正可滚动。
 *
 * 输出被 WS/合并管线切成任意 chunk，CSI 序列可能跨 chunk——内部保留尾部
 * 片段（TAIL_KEEP_CHARS），下一 chunk 到达时拼接后重新扫描。
 * 关闭序列（`...l`）一并跟踪，退出时复位。
 */
export interface MouseSgrSniffer {
  /** 应用当前是否启用了 SGR 鼠标上报（可接收 SGR 滚轮事件） */
  readonly enabled: boolean
  /** 喂入一段输出字节（写入 xterm 前调用） */
  feed(data: Uint8Array): void
  /** 复位（会话停止/断开时调用） */
  reset(): void
}

export function createMouseSgrSniffer(): MouseSgrSniffer {
  let tail = ''
  /** 各鼠标上报模式的当前开关（后到的 DECSET 覆盖前者） */
  const tracking = new Map<number, boolean>()
  /** 是否观察到过上报模式开关（决定 1006 是否可单独作为判据） */
  let trackingSeen = false
  let sgr = false
  const decoder = new TextDecoder()

  /**
   * 上报是否可用：
   * - 未观察到任何上报模式开关 → 以 1006 为准（历史兼容：部分应用只声明编码）
   * - 观察到过 → 以真实开关为准。这修的是反向漏判：应用启用鼠标上报后又显式
   *   关闭（`1000l` 等）但 1006 仍置位时，旧实现会长期停在 TUI 模式，手势持续
   *   发送被应用忽略的滚轮事件（表现为「滚不动」）
   */
  const mouseReportingActive = () =>
    !trackingSeen || MOUSE_TRACKING_MODES.some((mode) => tracking.get(mode) === true)

  return {
    get enabled() {
      return sgr && mouseReportingActive()
    },
    feed(data: Uint8Array) {
      const text = tail + decoder.decode(data)
      for (const m of text.matchAll(DECSET_QUESTION_RE)) {
        const on = m[2] === 'h'
        // 多参数：逐模式应用（ESC[?1000;1006h 同时开上报与 SGR 编码）
        for (const part of m[1].split(';')) {
          const mode = Number(part)
          if (!Number.isFinite(mode)) continue
          if (mode === MOUSE_SGR_MODE) {
            sgr = on
          } else if (MOUSE_TRACKING_MODES.includes(mode)) {
            trackingSeen = true
            tracking.set(mode, on)
          }
        }
      }
      tail = text.slice(-TAIL_KEEP_CHARS)
    },
    reset() {
      tail = ''
      tracking.clear()
      trackingSeen = false
      sgr = false
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

  // 节流状态（pendingScaled 为灵敏度分母缩放后的整数积压：events×DEN）
  let pendingScaled = 0
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
      if (pendingScaled === 0) return
      if (inflight) {
        // 上次发送仍在途：保留积压不清零（丢弃会丢失大量滚动量，
        // 体感「只能滚一屏多一点」），窗口到期后由下一次调度补发
        scheduleSend()
        return
      }
      // 只发整数个事件（缩放余量留在积压，累积到完整事件再发）；
      // 每窗口至多 MAX_WHEEL_EVENTS_PER_WINDOW 个：超出部分随下一窗口
      // 补发（不丢弃），摊平单帧批量跳跃
      const whole = Math.trunc(pendingScaled / WHEEL_SENSITIVITY_DEN)
      if (whole === 0) return
      const capped = Math.max(-MAX_WHEEL_EVENTS_PER_WINDOW, Math.min(whole, MAX_WHEEL_EVENTS_PER_WINDOW))
      pendingScaled -= capped * WHEEL_SENSITIVITY_DEN
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
          if (pendingScaled !== 0) scheduleSend()
        })
    }, WHEEL_THROTTLE_MS)
  }

  /**
   * 发送滚轮事件（TUI 模式）：手势行数经灵敏度折算后累积并节流合并，
   * 窗口到期生成 SGR 序列经 WS 送达 PTY，fire-and-forget。
   * 拖动与惯性甩动共用此入口，灵敏度单点生效
   */
  function sendWheel(deltaLines: number, col: number, row: number) {
    if (!isTuiMode.value || deltaLines === 0) return

    // 灵敏度折算：积压按分母缩放（events×DEN），行数×NUM 整数累加；
    // 积压上限：超过上限丢弃最旧部分（保留最新滚动意图），防长期拥塞无限堆积
    const maxScaled = MAX_PENDING_DELTA * WHEEL_SENSITIVITY_DEN
    pendingScaled = Math.max(-maxScaled, Math.min(pendingScaled + deltaLines * WHEEL_SENSITIVITY_NUM, maxScaled))
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
    pendingScaled = 0
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
