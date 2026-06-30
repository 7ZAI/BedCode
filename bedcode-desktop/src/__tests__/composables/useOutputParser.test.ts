import { describe, it, expect } from 'vitest'
import { useOutputParser } from '@/modules/shared/composables/useOutputParser'

describe('useOutputParser', () => {
  it('should initialize with empty state', () => {
    const { blocks, rawOutput } = useOutputParser()

    expect(blocks.value).toEqual([])
    expect(rawOutput.value).toBe('')
  })

  it('should parse plain text', () => {
    const { parseOutput, blocks } = useOutputParser()

    parseOutput('Hello, World!')

    expect(blocks.value.length).toBeGreaterThan(0)
    expect(blocks.value[0].type).toBe('text')
    expect(blocks.value[0].content).toBe('Hello, World!')
  })

  it('should parse code block', () => {
    const { parseOutput, blocks } = useOutputParser()

    parseOutput('```javascript\nconsole.log("test");\n```')

    const codeBlock = blocks.value.find(b => b.type === 'code')
    expect(codeBlock).toBeDefined()
    expect(codeBlock?.language).toBe('javascript')
    expect(codeBlock?.content).toContain('console.log')
  })

  it('should parse multiple outputs', () => {
    const { parseOutput, blocks } = useOutputParser()

    parseOutput('First line\n')
    parseOutput('Second line\n')

    expect(blocks.value.length).toBeGreaterThanOrEqual(1)
  })

  it('should accumulate raw output', () => {
    const { parseOutput, rawOutput } = useOutputParser()

    parseOutput('Line 1\n')
    parseOutput('Line 2\n')

    expect(rawOutput.value).toContain('Line 1')
    expect(rawOutput.value).toContain('Line 2')
  })

  it('should clear output', () => {
    const { parseOutput, clearOutput, blocks, rawOutput } = useOutputParser()

    parseOutput('Some content')
    clearOutput()

    expect(blocks.value).toEqual([])
    expect(rawOutput.value).toBe('')
  })

  it('should limit raw output size', () => {
    const { parseOutput, rawOutput } = useOutputParser()

    // Add a lot of content
    for (let i = 0; i < 10000; i++) {
      parseOutput('This is a long line of text\n')
    }

    // Raw output should be limited
    expect(rawOutput.value.length).toBeLessThan(200000)
  })

  it('should detect markdown content', () => {
    const { parseOutput, blocks } = useOutputParser()

    parseOutput('# Heading\n\nThis is **bold** text.')

    const textBlock = blocks.value.find(b => b.type === 'markdown')
    expect(textBlock).toBeDefined()
  })

  it('should get recent blocks', () => {
    const { parseOutput, getRecentBlocks } = useOutputParser()

    for (let i = 0; i < 100; i++) {
      parseOutput(`Block ${i}\n`)
    }

    const recent = getRecentBlocks(10)
    expect(recent.length).toBeLessThanOrEqual(10)
  })

  it('should strip ANSI codes', () => {
    const { parseOutput, blocks } = useOutputParser()

    parseOutput('\x1b[31mRed Text\x1b[0m')

    // Content should be clean (ANSI stripped)
    const textBlock = blocks.value.find(b => b.type === 'text')
    expect(textBlock?.content).not.toContain('\x1b')
  })
})
