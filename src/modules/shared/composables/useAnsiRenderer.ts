/**
 * ANSI 渲染 Composable
 *
 * 将 ANSI 转义序列转换为带样式的 HTML，用于终端输出渲染
 */

import { AnsiUp } from 'ansi_up'

// Re-export from model
import type { AnsiRenderOptions } from './model'
export type { AnsiRenderOptions }

// ANSI 转义序列正则表达式（标准 CSI 序列）
const ANSI_REGEX = /\x1b\[[0-9;]*[a-zA-Z]/g

// OSC 序列正则表达式（设置窗口标题等，不应显示）
// 格式: ESC ] ... BEL 或 ESC ] ... ST (ESC \)
const OSC_REGEX = /\x1b\][^\x07]*?\x07|\x1b\][^\x1b]*?\x1b\\/g

// 私有模式序列（如 bracketed paste mode: [?2026h/l）
// 格式: ESC [ ? digits h/l
const PRIVATE_MODE_REGEX = /\x1b\[\?\d+[hl]/g

// 不支持的 ANSI 序列（光标定位、屏幕清除等）
// AnsiUp 只处理颜色/样式序列，不处理光标移动
// 光标定位: ESC [ row ; col H 或 ESC [ H
// 光标移动: ESC [ A/B/C/D (上下左右)
// 清屏: ESC [ J, ESC [ K
// 保存/恢复光标: ESC [ s/u
// 模式设置: ESC [ ? ... h/l
const UNSUPPORTED_ANSI_REGEX = /\x1b\[(?:\d+;?\d*[HfABCDEFsuJK]|\d*[ABCDEFsuJK]|\?\d+[hl])/g

// 非 CSI 转义序列: ESC 后跟单个 ASCII 字符（非 [）
// 包括 DECSC (\x1b7 保存光标)、DECRC (\x1b8 恢复光标)、应用键区 (\x1b=/\x1b>) 等
// Claude Code 在 thinking 动画中使用这些序列
const NON_CSI_ESCAPE_REGEX = /\x1b[\x30-\x3f\x42-\x7e]/g

export function useAnsiRenderer(options?: AnsiRenderOptions) {
  const ansiUp = new AnsiUp()

  // 配置：默认使用内联样式（更简单）
  ansiUp.use_classes = options?.useClasses ?? false

  /**
   * 将 ANSI 文本转换为 HTML
   * @param text - 包含 ANSI 转义序列的文本
   * @returns HTML 字符串，span 元素带有 style 属性
   */
  function renderToHtml(text: string): string {
    // 预处理：过滤掉不支持的控制序列
    const cleanText = text
      // 移除 OSC 序列（窗口标题等）
      .replace(OSC_REGEX, '')
      // 移除私有模式序列（bracketed paste 等）
      .replace(PRIVATE_MODE_REGEX, '')
      // 移除不支持的 ANSI 序列（光标定位、移动、清屏等）
      .replace(UNSUPPORTED_ANSI_REGEX, '')
      // 移除非 CSI 转义序列（\x1b7/\x1b8 等 Claude Code 使用的序列）
      .replace(NON_CSI_ESCAPE_REGEX, '')

    // AnsiUp 处理颜色/样式序列
    return ansiUp.ansi_to_html(cleanText)
  }

  /**
   * 剥离 ANSI 序列，返回纯文本
   * @param text - 包含 ANSI 转义序列的文本
   * @returns 纯文本
   */
  function stripAnsi(text: string): string {
    return text
      .replace(OSC_REGEX, '')
      .replace(PRIVATE_MODE_REGEX, '')
      .replace(UNSUPPORTED_ANSI_REGEX, '')
      .replace(NON_CSI_ESCAPE_REGEX, '')
      .replace(ANSI_REGEX, '')
      .replace(/[\x00-\x08\x0B-\x0C\x0E-\x1F\x7F]/g, '')
  }

  /**
   * 解码 Base64 PTY 输出并渲染为 HTML
   * @param base64Data - Base64 编码的 PTY 输出
   * @returns HTML 字符串
   */
  function renderFromBase64(base64Data: string): string {
    try {
      // 正确解码 UTF-8
      const binary = atob(base64Data)
      const bytes = new Uint8Array(binary.length)
      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i)
      }
      const decoded = new TextDecoder('utf-8').decode(bytes)
      return renderToHtml(decoded)
    } catch {
      return ''
    }
  }

  return {
    renderToHtml,
    stripAnsi,
    renderFromBase64,
  }
}