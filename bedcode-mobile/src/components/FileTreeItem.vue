<template>
  <div class="tree-item">
    <div
      class="tree-item-row"
      :style="rowStyle"
      @click="handleClick"
      @touchstart="onTouchStart"
      @touchend="onTouchEnd"
      @touchmove="onTouchMove"
      @contextmenu.prevent="onContextMenu"
    >
      <!-- 文件夹加载中 spinner -->
      <svg
        v-if="node.type === 'folder' && node.loading"
        class="chevron spinning-icon"
        :width="iconSize"
        :height="iconSize"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
      </svg>
      <!-- 文件夹展开/折叠箭头 -->
      <svg
        v-else-if="node.type === 'folder'"
        class="chevron"
        :class="{ expanded: node.expanded }"
        :width="iconSize"
        :height="iconSize"
        viewBox="0 0 16 16"
        fill="currentColor"
      >
        <path d="M6 4l4 4-4 4" />
      </svg>
      <!-- 文件占位，保持对齐 -->
      <span v-else class="chevron-placeholder" :style="{ width: `${iconSize}px` }"></span>

      <!-- 图标 -->
      <FolderOpenIcon v-if="node.type === 'folder' && node.expanded" class="item-icon" :style="iconStyle" />
      <FolderClosedIcon v-else-if="node.type === 'folder'" class="item-icon" :style="iconStyle" />
      <FileIcon v-else class="item-icon" :style="iconStyle" :color="fileColor" />

      <!-- 名称 -->
      <span class="item-name" :style="{ fontSize: `${fontSize}px` }">{{ node.name }}</span>
    </div>

    <!-- 子节点 -->
    <div v-if="node.type === 'folder' && node.expanded" class="tree-item-children">
      <FileTreeItem
        v-for="(child, index) in node.children"
        :key="index"
        :node="child"
        :depth="depth + 1"
        :font-size="fontSize"
        @file-click="(name, path) => emit('file-click', name, path)"
        @long-press="(name, path) => emit('long-press', name, path)"
        @load-children="(n) => emit('load-children', n)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 文件树节点组件
 *
 * 支持懒加载：文件夹展开时 emit load-children 事件请求加载子节点
 */
import { computed } from 'vue'
import type { FileTreeNode } from '@/composables/useFileTree'
import FolderOpenIcon from './icons/FolderOpenIcon.vue'
import FolderClosedIcon from './icons/FolderClosedIcon.vue'
import FileIcon from './icons/FileIcon.vue'

const props = withDefaults(defineProps<{
  node: FileTreeNode
  depth: number
  fontSize?: number
}>(), {
  fontSize: 13,
})

const emit = defineEmits<{
  'file-click': [name: string, path: string]
  'long-press': [name: string, path: string]
  'load-children': [node: FileTreeNode]
}>()

// 基于 fontSize 的比例缩放因子（以 13px 为基准）
const scale = computed(() => props.fontSize / 13)

// 缩放后的尺寸
const iconSize = computed(() => Math.round(16 * scale.value))
const indentUnit = computed(() => Math.round(16 * scale.value))

const rowStyle = computed(() => ({
  paddingLeft: `${props.depth * indentUnit.value + Math.round(8 * scale.value)}px`,
  height: `${Math.round(32 * scale.value)}px`,
  gap: `${Math.round(4 * scale.value)}px`,
}))

const iconStyle = computed(() => ({
  width: `${iconSize.value}px`,
  height: `${iconSize.value}px`,
}))

// 文件扩展名对应的图标颜色
const EXTENSION_COLORS: Record<string, string> = {
  rs: '#dea584',
  ts: '#3178c6',
  js: '#f7df1e',
  vue: '#42b883',
  json: '#f5d142',
  toml: '#9c4221',
  md: '#519aba',
  css: '#563d7c',
  html: '#e34c26',
}

const fileColor = computed(() => {
  if (props.node.type === 'folder') return ''
  const ext = props.node.name.split('.').pop() || ''
  return EXTENSION_COLORS[ext] || 'var(--mobile-text-muted)'
})

// 长按检测
const LONG_PRESS_DURATION = 500
const LONG_PRESS_MOVE_THRESHOLD = 10
let longPressTimer: ReturnType<typeof setTimeout> | null = null
/** touch 长按已触发：用于抑制长按后的 click，以及 contextmenu 重复触发 */
let longPressTriggered = false
/** contextmenu 已处理：桌面右键/移动长按派生的 contextmenu 先于 timer 时防重 */
let contextMenuHandled = false
let touchStartX = 0
let touchStartY = 0

function onTouchStart(e: TouchEvent) {
  longPressTriggered = false
  contextMenuHandled = false
  touchStartX = e.touches[0].clientX
  touchStartY = e.touches[0].clientY
  longPressTimer = setTimeout(() => {
    // contextmenu 已触发过（移动端长按会派生该事件），避免二次复制弹窗
    if (contextMenuHandled) return
    longPressTriggered = true
    // 触觉反馈（受 vibrate 设置控制）
    const saved = localStorage.getItem('mobile-settings')
    const vibrateEnabled = saved ? JSON.parse(saved).vibrate ?? true : true
    if (vibrateEnabled && navigator.vibrate) {
      navigator.vibrate(30)
    }
    emit('long-press', props.node.name, props.node.path ?? props.node.name)
  }, LONG_PRESS_DURATION)
}

function onTouchEnd() {
  if (longPressTimer) {
    clearTimeout(longPressTimer)
    longPressTimer = null
  }
}

function onTouchMove(e: TouchEvent) {
  // 手指移动超过阈值时才取消长按，避免触摸抖动误取消
  const dx = e.touches[0].clientX - touchStartX
  const dy = e.touches[0].clientY - touchStartY
  if (Math.abs(dx) > LONG_PRESS_MOVE_THRESHOLD || Math.abs(dy) > LONG_PRESS_MOVE_THRESHOLD) {
    if (longPressTimer) {
      clearTimeout(longPressTimer)
      longPressTimer = null
    }
  }
}

function onContextMenu(e: Event) {
  // 桌面端右键菜单也触发长按
  e.preventDefault()
  // 移动端长按已通过 touch 路径触发过，跳过派生的 contextmenu 避免重复
  if (longPressTriggered) return
  contextMenuHandled = true
  emit('long-press', props.node.name, props.node.path ?? props.node.name)
}

function handleClick() {
  // 长按触发后忽略 click
  if (longPressTriggered) {
    longPressTriggered = false
    return
  }
  if (props.node.type === 'folder') {
    props.node.expanded = !props.node.expanded
    // 懒加载：展开时如果 children 未加载，请求加载
    if (props.node.expanded && props.node.children === undefined && !props.node.loading) {
      emit('load-children', props.node)
    }
  } else {
    emit('file-click', props.node.name, props.node.path ?? props.node.name)
  }
}
</script>

<style scoped>
.tree-item-row {
  display: flex;
  align-items: center;
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  transition: background-color 0.15s ease;
}

.tree-item-row:active {
  background: var(--mobile-accent-muted);
}

.chevron {
  flex-shrink: 0;
  color: var(--mobile-text-muted);
  transition: transform 0.2s ease;
  transform: rotate(0deg);
}

.chevron.expanded {
  transform: rotate(90deg);
}

.chevron-placeholder {
  flex-shrink: 0;
}

.item-icon {
  flex-shrink: 0;
}

.item-name {
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tree-item-children {
  overflow: hidden;
}

.spinning-icon {
  animation: spin 1s linear infinite;
  color: var(--mobile-text-muted);
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
