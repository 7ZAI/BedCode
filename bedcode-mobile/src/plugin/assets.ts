/**
 * 资源文件 → asset protocol URL（全宿主共用一份实现）
 * -----------------------------------------------------------------------------
 * 两类来源都收口在这里：
 *   ① 插件包内资源：插件目录 + 包内相对路径（extensionPath + main / icon）；
 *   ② 宿主导入的用户资源：壳 UI 选图后由宿主命令拷进 $APPDATA 的图片（壁纸等）。
 * 两者都要经 Tauri 的 asset protocol 才能被 WebView 取到（Android 上即
 * `http://asset.localhost/...`），而 asset scope 正是 $RESOURCE/$APPCONFIG/$APPDATA。
 *
 * 为什么不写在壳里：壳有自己的源码锁（`src/__tests__/shell/shellConstraintLocks.test.ts`
 * L2「不经裸 invoke」）——宿主调用一律走既有封装，壳内直接 import `@tauri-apps/api/core`
 * 会破坏那条红线。因此拼接与导入都收口在本模块：loader（前端模块）与壳共用一份实现，
 * 两处各写一遍迟早会在转义/分隔符上分叉。
 */

import { convertFileSrc, invoke } from '@tauri-apps/api/core'

/** 把插件包内相对路径拼成 asset protocol URL（反斜杠统一成 /，兼容 Windows 下的目录串） */
export function toPluginAssetUrl(extensionPath: string, relativePath: string): string {
  const filePath = `${extensionPath}/${relativePath}`.replace(/\\/g, '/')
  return convertFileSrc(filePath)
}

/** 把宿主侧绝对路径转成 asset protocol URL（路径分隔符统一，兼容 Windows 目录串） */
export function toFileAssetUrl(absolutePath: string): string {
  return convertFileSrc(absolutePath.replace(/\\/g, '/'))
}

/**
 * 让用户从系统相册 / 文件选择器挑一张图片，作为壳的自定义壁纸
 *
 * 选图与落盘都由宿主完成（前端零资源访问红线，AGENTS §6）：这里只拿到宿主
 * 拷贝进 `$APPDATA` 后的绝对路径，渲染时再经 {@link toFileAssetUrl} 取图。
 *
 * @returns 落盘后的绝对路径；`null` = 用户取消（调用方不作为错误处理）
 */
export async function importShellWallpaper(): Promise<string | null> {
  return await invoke('shell_import_image', { namespace: 'wallpaper' })
}
