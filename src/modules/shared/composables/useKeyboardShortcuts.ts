import { onMounted, onUnmounted } from 'vue'

// Re-export from model
import type { Shortcut } from './model'
export type { Shortcut }

/**
 * 注册键盘快捷键
 *
 * 在组件挂载时绑定全局 keydown 事件，卸载时自动解绑
 */
export function useKeyboardShortcuts(shortcuts: Shortcut[]) {
  function handleKeydown(event: KeyboardEvent) {
    for (const sc of shortcuts) {
      // 检查修饰键匹配
      const ctrlOrMeta = sc.ctrl || sc.meta
      const matchesMod = ctrlOrMeta
        ? (event.ctrlKey || event.metaKey)
        : (!event.ctrlKey && !event.metaKey)
      const matchesShift = sc.shift ? event.shiftKey : !event.shiftKey
      const matchesKey = event.key.toLowerCase() === sc.key.toLowerCase()

      if (matchesMod && matchesShift && matchesKey) {
        // 跳过输入聚焦的元素（除非明确允许）
        if (!sc.ignoreInput) {
          const target = event.target as HTMLElement
          const tag = target.tagName.toLowerCase()
          if (tag === 'input' || tag === 'textarea' || tag === 'select' || target.isContentEditable) {
            continue
          }
        }

        event.preventDefault()
        sc.handler?.()
        return
      }
    }
  }

  onMounted(() => document.addEventListener('keydown', handleKeydown))
  onUnmounted(() => document.removeEventListener('keydown', handleKeydown))
}
