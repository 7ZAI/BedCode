/**
 * 事件监听封装（useDesktopCommands 拆分产物）
 *
 * 设备连接 / 断开事件的 Tauri listen 注册与统一清理（组件卸载时调用
 * cleanupEventListeners 释放监听，避免重复注册泄漏）。
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

let unlistenDeviceConnected: UnlistenFn | null = null
let unlistenDeviceDisconnected: UnlistenFn | null = null

/** 监听设备连接事件 */
export async function onDeviceConnected(callback: (event: any) => void): Promise<() => void> {
  unlistenDeviceConnected = await listen('device-connected', callback)
  return unlistenDeviceConnected
}

/** 监听设备断开事件 */
export async function onDeviceDisconnected(callback: (event: any) => void): Promise<() => void> {
  unlistenDeviceDisconnected = await listen('device-disconnected', callback)
  return unlistenDeviceDisconnected
}

/** 清理所有事件监听 */
export function cleanupEventListeners() {
  unlistenDeviceConnected?.()
  unlistenDeviceDisconnected?.()
}
