//! Tauri IPC 调用超时机制
//!
//! 防止因后端崩溃或死锁导致前端 Promise 永久挂起。
//! 超时失败承载为 `UserError('host.invoke.timeout', { seconds })`（ADR 0030 错误信封），
//! 不再携带命令名 / 技术详情——用户面由 showUserError 统一呈现「操作超时，请重试」。

import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { IPC_TIMEOUT_CODE, UserError } from '@/utils/userError'

const DEFAULT_TIMEOUT_MS = 30_000

/**
 * 带超时的 Tauri IPC 调用
 *
 * 超时后 Promise reject 为 `UserError('host.invoke.timeout', { seconds })`（params 仅用户
 * 安全值：秒数），调用方可用 `showUserError(e, { retry })` 展示友好提示 + 重试。
 */
export async function invokeWithTimeout<T>(
  cmd: string,
  args?: Record<string, unknown>,
  timeoutMs: number = DEFAULT_TIMEOUT_MS,
): Promise<T> {
  const invokePromise = tauriInvoke<T>(cmd, args)

  const timeoutPromise = new Promise<never>((_, reject) => {
    setTimeout(() => {
      reject(new UserError(IPC_TIMEOUT_CODE, { seconds: Math.round(timeoutMs / 1000) }))
    }, timeoutMs)
  })

  return Promise.race([invokePromise, timeoutPromise])
}

/**
 * 不带超时的 invoke（直接透传，用于对响应时间不敏感的简单查询）
 */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args)
}