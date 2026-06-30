//! Tauri IPC 调用超时机制
//!
//! 防止因后端崩溃或死锁导致前端 Promise 永久挂起

import { invoke as tauriInvoke } from '@tauri-apps/api/core'

const DEFAULT_TIMEOUT_MS = 30_000

/** 超时专用错误类，便于前端区分超时与其他错误 */
export class InvokeTimeoutError extends Error {
  constructor(cmd: string, timeoutMs: number) {
    super(`common.errorCode.ipcTimeout`)
    this.name = 'InvokeTimeoutError'
  }
}

/**
 * 带超时的 Tauri IPC 调用
 *
 * 超时后 Promise 会 reject 并抛出 InvokeTimeoutError，
 * 调用方可据此展示明确的超时提示或触发重试。
 */
export async function invokeWithTimeout<T>(
  cmd: string,
  args?: Record<string, unknown>,
  timeoutMs: number = DEFAULT_TIMEOUT_MS,
): Promise<T> {
  const invokePromise = tauriInvoke<T>(cmd, args)

  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => {
      reject(new InvokeTimeoutError(cmd, timeoutMs))
    }, timeoutMs)
  })

  return Promise.race([invokePromise, timeoutPromise])
}

/**
 * 不带超时的 invoke（直接透传，用于对响应时间不敏感的简单查询）
 */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return tauriInvoke<T>(cmd, args)
}
