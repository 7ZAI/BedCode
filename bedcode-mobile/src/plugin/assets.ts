/**
 * 插件包内资源 → asset protocol URL
 * -----------------------------------------------------------------------------
 * 插件的前端模块、图标等都以「插件目录 + 包内相对路径」的形式下发（extensionPath + main /
 * icon）。要真正取到文件必须经 Tauri 的 asset protocol（Android 上即
 * `http://tauri.localhost/...`）。
 *
 * 为什么不写在壳里：壳有自己的源码锁（`src/__tests__/shell/shellConstraintLocks.test.ts`
 * L2「不经裸 invoke」）——宿主调用一律走既有封装，壳内直接 import `@tauri-apps/api/core`
 * 会破坏那条红线。因此 URL 拼接收口在本模块：loader（前端模块）与壳适配器共用一份实现，
 * 两处各写一遍迟早会在转义/分隔符上分叉。
 */

import { convertFileSrc } from '@tauri-apps/api/core'

/** 把插件包内相对路径拼成 asset protocol URL（反斜杠统一成 /，兼容 Windows 下的目录串） */
export function toPluginAssetUrl(extensionPath: string, relativePath: string): string {
  const filePath = `${extensionPath}/${relativePath}`.replace(/\\/g, '/')
  return convertFileSrc(filePath)
}
