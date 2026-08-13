/**
 * 终端输入标记 Composable
 *
 * 追踪用户在终端中的每一次输入（回车提交时记录），供终端输入导航条
 * （TerminalInputRail）渲染"横线标记"使用：一根横线 = 一次输入。
 *
 * 位置校正依赖 xterm IMarker：registerMarker(0) 创建光标行标记，其 line
 * 属性随 scrollback trim（环形淘汰）自动递减校正，line < 0 表示该行已被
 * 淘汰出缓冲，渲染时应过滤。淘汰最旧记录（FIFO）时 dispose 对应 marker，
 * 避免泄漏。
 */

import { ref, computed } from 'vue'
import type { IMarker, Terminal } from '@xterm/xterm'

/** 一次输入的位置标记（内部记录：持有 IMarker 引用，line 随缓冲变化） */
export interface InputMarkerRecord {
  id: number
  /** xterm 标记：line 为只读且随 scrollback trim 自动校正 */
  marker: IMarker
  /** 输入文本（不含提示符；多行粘贴每行一条记录） */
  text: string
}

/** 渲染层使用的轻量标记（line 为当前 buffer 绝对行号快照） */
export interface InputMarker {
  id: number
  line: number
  text: string
}

export interface InputMarkersOptions {
  /** 最大横线数量（默认 10），超出后淘汰最旧 */
  maxMarkers?: number
}

export function useTerminalInputMarkers(options?: InputMarkersOptions) {
  const maxMarkers = options?.maxMarkers ?? 10
  const records = ref<InputMarkerRecord[]>([])
  let nextId = 1

  /**
   * 记录一次输入：在终端当前光标行创建 marker。
   * @param terminal xterm 实例（registerMarker 在 alternate buffer 下可能失败）
   * @param text 输入文本
   */
  function record(terminal: Terminal, text: string): void {
    const marker = terminal.registerMarker(0)
    if (!marker) return
    records.value.push({ id: nextId++, marker, text })
    // FIFO 淘汰最旧记录，dispose 释放 xterm 内部引用
    while (records.value.length > maxMarkers) {
      const removed = records.value.shift()
      removed?.marker.dispose()
    }
  }

  /** 可见标记：过滤已淘汰（line < 0）行，按记录时间正序，截取最近 maxMarkers 条 */
  const visibleMarkers = computed<InputMarker[]>(() =>
    records.value
      .filter((r) => r.marker.line >= 0)
      .map((r) => ({ id: r.id, line: r.marker.line, text: r.text })),
  )

  /** 清空全部标记（清屏 / 会话切换 / 组件卸载时调用） */
  function clear(): void {
    for (const r of records.value) {
      r.marker.dispose()
    }
    records.value = []
  }

  return {
    records,
    visibleMarkers,
    record,
    clear,
    maxMarkers,
  }
}
