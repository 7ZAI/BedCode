/**
 * 全局终端历史缓存管理器
 *
 * 不监听 PTY 输出，只存储历史数据（原始 ANSI 字节分块）
 * 组件负责同步写入全局缓存
 *
 * 历史：早期版本为追踪行数维护隐藏 xterm 实例，但行数从未被任何调用方
 * 读取，且高频输出场景下每次输出被解析两遍（显示实例 + 隐藏实例），
 * CPU 开销翻倍。现只保留纯字节缓存，ANSI 解析只发生在显示实例中。
 */

import { computed, type Ref } from 'vue'

// 会话历史缓存：sessionId -> 原始输出字节分块
const sessionHistoryCache = new Map<string, Uint8Array[]>()

// 缓存字节上限（约 5MB，超出后丢弃旧数据）
// xterm scrollback 负责行数限制，cache 只需控制内存总量
const MAX_CACHE_BYTES = 5 * 1024 * 1024

// 追踪每个会话缓存的总字节数
const sessionCacheSizes = new Map<string, number>()

/**
 * 初始化会话的历史缓存
 *
 * @param sessionId - 会话 ID
 */
export function initSessionCache(sessionId: string): void {
  if (sessionHistoryCache.has(sessionId)) {
    return
  }

  sessionHistoryCache.set(sessionId, [])
  sessionCacheSizes.set(sessionId, 0)
}

/**
 * 追加输出数据到历史缓存
 */
export function appendOutput(sessionId: string, data: Uint8Array): void {
  const cache = sessionHistoryCache.get(sessionId)

  if (!cache) {
    console.warn('[TerminalCache] Session cache not initialized:', sessionId)
    return
  }

  // 追加到缓存
  cache.push(data)

  // 更新字节计数
  const currentSize = (sessionCacheSizes.get(sessionId) || 0) + data.byteLength
  sessionCacheSizes.set(sessionId, currentSize)

  // 内存限制：超出上限时丢弃旧数据
  // 丢弃到 70% 水位，避免频繁触发截断
  if (currentSize > MAX_CACHE_BYTES) {
    const targetSize = MAX_CACHE_BYTES * 0.7
    let removedSize = 0
    while (cache.length > 1 && (currentSize - removedSize) > targetSize) {
      const removed = cache.shift()
      if (removed) {
        removedSize += removed.byteLength
      }
    }
    sessionCacheSizes.set(sessionId, currentSize - removedSize)
  }
}

/**
 * 获取会话的历史输出（用于恢复终端显示），合并为单块字节
 *
 * @returns 合并后的字节数据；无历史时返回 null
 */
export function getHistoryOutput(sessionId: string): Uint8Array | null {
  const cache = sessionHistoryCache.get(sessionId)
  if (!cache || cache.length === 0) {
    return null
  }

  let totalBytes = 0
  for (const chunk of cache) totalBytes += chunk.byteLength

  const combined = new Uint8Array(totalBytes)
  let offset = 0
  for (const chunk of cache) {
    combined.set(chunk, offset)
    offset += chunk.byteLength
  }
  return combined
}

/**
 * 清除会话的历史缓存
 */
export function clearHistoryCache(sessionId: string): void {
  const cache = sessionHistoryCache.get(sessionId)
  if (cache) {
    cache.length = 0
  }
  sessionCacheSizes.set(sessionId, 0)
}

/**
 * 销毁会话的历史缓存（停止会话时调用）
 */
export function destroySessionCache(sessionId: string): void {
  sessionHistoryCache.delete(sessionId)
  sessionCacheSizes.delete(sessionId)
}

/**
 * 检查会话是否有历史缓存
 */
export function hasSessionCache(sessionId: string): boolean {
  return sessionHistoryCache.has(sessionId)
}

/**
 * 清理所有缓存
 */
export function cleanupAllCaches(): void {
  sessionHistoryCache.clear()
  sessionCacheSizes.clear()
}

/**
 * Composable: 使用终端历史缓存
 *
 * @param sessionId - 会话 ID（字符串或 Ref，支持响应式切换）
 */
export function useTerminalHistory(sessionId: string | Ref<string>) {
  const sessionIdRef = computed(() => {
    if (typeof sessionId === 'string') return sessionId
    return sessionId.value
  })

  return {
    init: () => initSessionCache(sessionIdRef.value),
    append: (data: Uint8Array) => appendOutput(sessionIdRef.value, data),
    getHistory: () => getHistoryOutput(sessionIdRef.value),
    clear: () => clearHistoryCache(sessionIdRef.value),
    destroy: () => destroySessionCache(sessionIdRef.value),
    hasCache: () => hasSessionCache(sessionIdRef.value),
  }
}
