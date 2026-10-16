/**
 * Clipboard Utilities - 跨平台剪贴板写入
 *
 * Android WebView 中 navigator.clipboard.writeText() 可能因权限问题失败，
 * 使用 document.execCommand('copy') 作为 fallback
 */

/**
 * 写入文本到剪贴板
 *
 * 优先使用 Clipboard API，失败时 fallback 到 execCommand('copy')
 */
export async function writeClipboardText(text: string): Promise<void> {
  // 优先使用现代 Clipboard API
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text)
      return
    } catch {
      // Clipboard API 失败，fallback 到 execCommand
    }
  }

  // Fallback：创建临时 textarea + execCommand('copy')
  const textarea = document.createElement('textarea')
  textarea.value = text
  // 固定定位到屏幕外，避免视觉闪烁
  textarea.style.position = 'fixed'
  textarea.style.left = '-9999px'
  textarea.style.top = '-9999px'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  try {
    const ok = document.execCommand('copy')
    if (!ok) {
      throw new Error('execCommand copy returned false')
    }
  } finally {
    document.body.removeChild(textarea)
  }
}
