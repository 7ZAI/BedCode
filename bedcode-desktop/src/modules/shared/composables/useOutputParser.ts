import { ref } from 'vue'

// Re-export from model
import type { OutputBlock } from './model'
export type { OutputBlock }

export function useOutputParser() {
  const blocks = ref<OutputBlock[]>([])
  const rawOutput = ref<string>('')
  const maxBlocks = 500

  function parseOutput(data: string) {
    // Append to raw output
    rawOutput.value += data

    // Limit raw output size
    if (rawOutput.value.length > 100000) {
      rawOutput.value = rawOutput.value.slice(-80000)
    }

    // Parse into blocks
    const newBlocks = parseIntoBlocks(data)

    for (const block of newBlocks) {
      addBlock(block)
    }
  }

  function parseIntoBlocks(data: string): OutputBlock[] {
    const result: OutputBlock[] = []
    const timestamp = Date.now()

    // Check for ANSI codes and strip them for now
    const cleanData = stripAnsi(data)

    // Detect code blocks
    const codeBlockRegex = /```(\w+)?\n([\s\S]*?)```/g
    let lastIndex = 0
    let match

    while ((match = codeBlockRegex.exec(cleanData)) !== null) {
      // Add text before code block
      if (match.index > lastIndex) {
        const text = cleanData.slice(lastIndex, match.index).trim()
        if (text) {
          result.push({
            id: generateId(),
            type: detectTextType(text),
            content: text,
            timestamp
          })
        }
      }

      // Add code block
      result.push({
        id: generateId(),
        type: 'code',
        content: match[2].trim(),
        language: match[1] || 'text',
        timestamp
      })

      lastIndex = match.index + match[0].length
    }

    // Add remaining text
    if (lastIndex < cleanData.length) {
      const text = cleanData.slice(lastIndex).trim()
      if (text) {
        result.push({
          id: generateId(),
          type: detectTextType(text),
          content: text,
          timestamp
        })
      }
    }

    // If no special blocks found, treat as plain text
    if (result.length === 0 && cleanData.trim()) {
      result.push({
        id: generateId(),
        type: 'text',
        content: cleanData.trim(),
        timestamp
      })
    }

    return result
  }

  function detectTextType(text: string): OutputBlock['type'] {
    // Check for markdown headers
    if (/^#{1,6}\s/.test(text)) {
      return 'markdown'
    }

    // Check for list items
    if (/^[\-\*\+]\s/.test(text) || /^\d+\.\s/.test(text)) {
      return 'markdown'
    }

    // Check for tool use patterns
    if (text.includes('Tool:') || text.includes('Using tool:')) {
      return 'tool_use'
    }

    // Check for error patterns
    if (/error|Error|ERROR|failed|Failed|FAILED/.test(text)) {
      return 'error'
    }

    return 'text'
  }

  function stripAnsi(str: string): string {
    // 移除 ANSI 转义序列：覆盖 CSI、OSC、私有模式、非 CSI 转义等全部类型
    return str
      // OSC 序列: ESC ] ... (BEL|ST)
      .replace(/\x1b\][^\x07]*?\x07|\x1b\][^\x1b]*?\x1b\\/g, '')
      // 私有模式序列: ESC [ ? digits h/l
      .replace(/\x1b\[\?\d+[hl]/g, '')
      // 光标定位/移动/清屏等不支持序列
      .replace(/\x1b\[(?:\d+;?\d*[HfABCDEFsuJK]|\d*[ABCDEFsuJK])/g, '')
      // 非 CSI 转义序列: \x1b7/\x1b8 (DECSC/DECRC) 等
      .replace(/\x1b[\x30-\x3f\x42-\x7e]/g, '')
      // 标准 CSI SGR 序列
      // eslint-disable-next-line no-control-regex
      .replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
      // 剩余控制字符（如 \x1b 孤立的 ESC）
      .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F\x7F]/g, '')
  }

  function addBlock(block: OutputBlock) {
    blocks.value.push(block)

    // Limit blocks count
    if (blocks.value.length > maxBlocks) {
      blocks.value = blocks.value.slice(-maxBlocks)
    }
  }

  function generateId(): string {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
  }

  function clearOutput() {
    blocks.value = []
    rawOutput.value = ''
  }

  /**
   * 增量追加输出：仅解析新数据并追加到现有 blocks
   * 用于增量输出场景，避免每次清空全部重新解析
   */
  function appendOutput(data: string) {
    if (!data) return

    // Append to raw output
    rawOutput.value += data

    // Limit raw output size
    if (rawOutput.value.length > 100000) {
      rawOutput.value = rawOutput.value.slice(-80000)
    }

    // Parse into blocks and append
    const newBlocks = parseIntoBlocks(data)
    for (const block of newBlocks) {
      addBlock(block)
    }
  }

  function getRecentBlocks(count: number = 50): OutputBlock[] {
    return blocks.value.slice(-count)
  }

  return {
    blocks,
    rawOutput,
    parseOutput,
    appendOutput,
    clearOutput,
    getRecentBlocks
  }
}
