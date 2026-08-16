/**
 * Sidebar Collapse State — 侧边栏折叠/展开状态管理
 *
 * 提供 collapsed 状态、toggle 方法、拖拽 resize 逻辑
 */
import { ref } from 'vue'

const collapsed = ref(false)

/** 侧边栏宽度阈值：拖拽低于此值自动折叠，高于此值自动展开 */
const COLLAPSE_THRESHOLD = 120
const EXPANDED_WIDTH = 240
const COLLAPSED_WIDTH = 56

/** 切换折叠/展开 */
export function toggleSidebar() {
  collapsed.value = !collapsed.value
}

/** 折叠侧边栏 */
export function collapseSidebar() {
  collapsed.value = true
}

/** 展开侧边栏 */
export function expandSidebar() {
  collapsed.value = false
}

/**
 * 侧边栏拖拽 resize composable
 *
 * 在侧边栏右边缘拖拽可调整宽度，拖到阈值以下自动折叠
 */
export function useSidebarResize() {
  const isResizing = ref(false)
  const dragWidth = ref(EXPANDED_WIDTH)

  function onResizeStart(e: MouseEvent) {
    // 只响应左键
    if (e.button !== 0) return
    isResizing.value = true
    dragWidth.value = collapsed.value ? COLLAPSED_WIDTH : EXPANDED_WIDTH

    const startX = e.clientX
    const startWidth = dragWidth.value

    function onMouseMove(ev: MouseEvent) {
      const delta = ev.clientX - startX
      const newWidth = startWidth + delta
      dragWidth.value = Math.max(COLLAPSED_WIDTH, Math.min(newWidth, 400))
    }

    function onMouseUp() {
      isResizing.value = false
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)

      // 根据最终宽度决定折叠/展开
      if (dragWidth.value < COLLAPSE_THRESHOLD) {
        collapsed.value = true
        dragWidth.value = COLLAPSED_WIDTH
      } else {
        collapsed.value = false
        dragWidth.value = EXPANDED_WIDTH
      }
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
  }

  return { isResizing, dragWidth, onResizeStart }
}

export { collapsed, COLLAPSED_WIDTH, EXPANDED_WIDTH }
