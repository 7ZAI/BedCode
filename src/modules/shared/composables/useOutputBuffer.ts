import { ref, onUnmounted } from 'vue'

export interface BufferedOutput {
  text: string
  timestamp: number
}

export function useOutputBuffer(flushInterval: number = 50, maxBufferSize: number = 10000) {
  const buffer = ref<string[]>([])
  const isFlushing = ref(false)
  let flushTimer: ReturnType<typeof setTimeout> | null = null
  let totalSize = 0

  function append(data: string) {
    buffer.value.push(data)
    totalSize += data.length

    // Trim if exceeding max size
    while (totalSize > maxBufferSize && buffer.value.length > 1) {
      const removed = buffer.value.shift()
      if (removed) {
        totalSize -= removed.length
      }
    }

    // Schedule flush
    if (!flushTimer) {
      flushTimer = setTimeout(flush, flushInterval)
    }
  }

  function flush(): BufferedOutput | null {
    if (buffer.value.length === 0) {
      return null
    }

    const text = buffer.value.join('')
    buffer.value = []

    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }

    return {
      text,
      timestamp: Date.now()
    }
  }

  function clear() {
    buffer.value = []
    totalSize = 0
    if (flushTimer) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
  }

  function getSize(): number {
    return totalSize
  }

  onUnmounted(() => {
    if (flushTimer) {
      clearTimeout(flushTimer)
    }
  })

  return {
    buffer,
    isFlushing,
    append,
    flush,
    clear,
    getSize
  }
}
