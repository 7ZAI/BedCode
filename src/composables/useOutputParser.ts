import { ref } from 'vue'

export interface OutputBlock {
  id: string
  type: 'text' | 'markdown' | 'code' | 'progress' | 'error' | 'tool_use'
  content: string
  language?: string
  percent?: number
  message?: string
  timestamp: number
}

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
    // eslint-disable-next-line no-control-regex
    return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
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

  function getRecentBlocks(count: number = 50): OutputBlock[] {
    return blocks.value.slice(-count)
  }

  return {
    blocks,
    rawOutput,
    parseOutput,
    clearOutput,
    getRecentBlocks
  }
}
