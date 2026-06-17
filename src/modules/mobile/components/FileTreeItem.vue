<template>
  <div class="tree-item">
    <div
      class="tree-item-row"
      :style="{ paddingLeft: `${depth * 16 + 8}px` }"
      @click="handleClick"
    >
      <!-- 文件夹展开/折叠箭头 -->
      <svg
        v-if="node.type === 'folder'"
        class="chevron"
        :class="{ expanded: node.expanded }"
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="currentColor"
      >
        <path d="M6 4l4 4-4 4" />
      </svg>
      <!-- 文件占位，保持对齐 -->
      <span v-else class="chevron-placeholder"></span>

      <!-- 图标 -->
      <FolderOpenIcon v-if="node.type === 'folder' && node.expanded" class="item-icon" />
      <FolderClosedIcon v-else-if="node.type === 'folder'" class="item-icon" />
      <FileIcon v-else class="item-icon" :color="fileColor" />

      <!-- 名称 -->
      <span class="item-name">{{ node.name }}</span>
    </div>

    <!-- 子节点 -->
    <div v-if="node.type === 'folder' && node.expanded" class="tree-item-children">
      <FileTreeItem
        v-for="(child, index) in node.children"
        :key="index"
        :node="child"
        :depth="depth + 1"
        @file-click="(name) => emit('file-click', name)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { FileTreeNode } from '@/modules/mobile/composables/useFileTree'
import FolderOpenIcon from './icons/FolderOpenIcon.vue'
import FolderClosedIcon from './icons/FolderClosedIcon.vue'
import FileIcon from './icons/FileIcon.vue'

const props = defineProps<{
  node: FileTreeNode
  depth: number
}>()

const emit = defineEmits<{
  'file-click': [name: string]
}>()

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

function handleClick() {
  if (props.node.type === 'folder') {
    props.node.expanded = !props.node.expanded
  } else {
    emit('file-click', props.node.name)
  }
}
</script>

<style scoped>
.tree-item-row {
  display: flex;
  align-items: center;
  height: 32px;
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  transition: background-color 0.15s ease;
  gap: 4px;
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
  width: 16px;
}

.item-icon {
  flex-shrink: 0;
  width: 16px;
  height: 16px;
}

.item-name {
  font-size: 13px;
  color: var(--mobile-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tree-item-children {
  overflow: hidden;
}
</style>
