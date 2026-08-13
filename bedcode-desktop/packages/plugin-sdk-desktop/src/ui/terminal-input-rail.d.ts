import type { DefineComponent } from 'vue'

/** 一次用户输入的位置标记（宿主端由 useTerminalInputMarkers 等 composable 产出，结构相同） */
export interface InputMarker {
  id: number
  /** buffer 绝对行号（xterm 坐标，随 scrollback 淘汰自动校正，-1 = 已淘汰） */
  line: number
  /** 输入文本（不含提示符；多行粘贴取首行） */
  text: string
}

/** TerminalInputRail 组件 props */
export interface TerminalInputRailProps {
  /** 输入标记（已过滤 line >= 0，按时间正序，最多 maxMarkers 条） */
  markers: InputMarker[]
  /** 当前 buffer 总行数（横线位置百分比 = line / (bufferLength - 1)） */
  bufferLength: number
  /** alternate buffer（vim 等 TUI 全屏程序）时隐藏 */
  isAltBuffer?: boolean
}

/**
 * 终端输入导航条（SDK 共享 UI）
 *
 * 一根横线 = 一次用户输入；默认态为右侧竖直居中、固定高度的透明横线条带，
 * hover 展开为输入列表卡片，点击行/横线 emit `navigate(line)`。
 */
export declare const TerminalInputRail: DefineComponent<TerminalInputRailProps>

export default TerminalInputRail