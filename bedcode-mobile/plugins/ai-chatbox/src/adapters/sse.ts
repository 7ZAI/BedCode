/**
 * SSE 前端缓冲解析器（传输层）
 *
 * 宿主 raw 模式逐网络 chunk 推送原始字节，一条 SSE 事件可能被切碎到
 * 多个 chunk（也可能一包多事件）。本模块按事件分隔符（空行）切分并抽取
 * `data:` 行内容，产出完整的 data 载荷；JSON 语义解析由各 adapter 的
 * parseStreamEvent 完成，职责单点。
 */
export class SseBuffer {
  private buffer = ''

  /** 追加一段原始字节，返回本次产出的完整 data 载荷（可能为空数组） */
  push(chunk: string): string[] {
    this.buffer += chunk
    return this.extractEvents()
  }

  /** 流结束时调用：残留的未闭合事件按最后一条 data 载荷产出（清空缓冲） */
  flush(): string[] {
    const events = this.extractEvents()
    const rest = this.buffer.trim()
    if (!rest) return events
    this.buffer = ''
    const data = extractDataLines(rest)
    return data ? [...events, data] : events
  }

  private extractEvents(): string[] {
    // 归一化行尾（SSE 规范允许 CRLF / 单独 CR），事件以空行分隔
    let text = this.buffer.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
    const events: string[] = []
    let sep: number
    while ((sep = text.indexOf('\n\n')) !== -1) {
      const eventText = text.slice(0, sep)
      text = text.slice(sep + 2)
      const data = extractDataLines(eventText)
      if (data) events.push(data)
    }
    this.buffer = text
    return events
  }
}

/** 抽取事件文本中的 data: 行（多行 data 按 \n 拼接，符合 SSE 规范）；无 data 行返回 null */
function extractDataLines(eventText: string): string | null {
  const lines: string[] = []
  for (const line of eventText.split('\n')) {
    if (line.startsWith('data:')) {
      const payload = line.slice(5).trim()
      if (payload) lines.push(payload)
    }
  }
  return lines.length ? lines.join('\n') : null
}
