<template>
  <!-- 图片图标：相对插件目录的资源路径 -->
  <div v-if="kind === 'image'" class="icon-tile overflow-hidden" :class="sizeClass">
    <img v-if="!imgFailed" :src="imageSrc" :alt="name" class="w-full h-full object-cover" @error="imgFailed = true" />
    <LetterAvatar v-else :name="name" :seed="pluginId" :size="size" />
  </div>

  <!-- 内联 SVG：manifest.icon 直接携带 <svg> 标记 -->
  <div v-else-if="kind === 'svg'" class="icon-tile svg-tile" :class="sizeClass" v-html="sanitizedSvg"></div>

  <!-- 回退：渐变字母头像 -->
  <LetterAvatar v-else :name="name" :seed="pluginId" :size="size" />
</template>

<script setup lang="ts">
/**
 * PluginIcon - 插件图标（桌面端）
 *
 * 三级回退逻辑：
 * 1. manifest.icon 为图片路径（png/jpg/webp/gif/svg）→ 经 asset protocol 加载
 * 2. manifest.icon 为内联 <svg> 标记 → 消毒后直接渲染
 * 3. 无 icon 或图片加载失败 → 按插件 id 哈希生成渐变字母头像
 *
 * 桌面端不渲染 emoji（区别于移动端），直接回退字母头像以保持 Workbench 风格一致性
 */
import { computed, ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import LetterAvatar from './LetterAvatar.vue'

const props = withDefaults(
  defineProps<{
    /** manifest.icon：内联 <svg> 标记或相对插件目录的图片路径 */
    icon?: string
    /** 插件名（字母头像取首字符） */
    name: string
    /** 插件 id（字母头像渐变配色的哈希种子） */
    pluginId: string
    /** 插件目录路径，图片 icon 相对此目录解析 */
    extensionPath?: string
    /** 图标尺寸 */
    size?: 'md' | 'lg'
  }>(),
  { icon: '', extensionPath: '', size: 'md' }
)

const imgFailed = ref(false)

// 插件更新换图标后重置失败标记，避免停留在字母头像回退
watch(() => [props.icon, props.extensionPath], () => {
  imgFailed.value = false
})

const IMAGE_EXT_RE = /\.(png|jpe?g|webp|gif|svg)$/i

/** 图标类型判定：图片路径 / 内联 SVG / 回退字母头像 */
const kind = computed<'image' | 'svg' | 'letter'>(() => {
  const icon = props.icon?.trim()
  if (!icon) return 'letter'
  if (icon.startsWith('<svg')) return 'svg'
  if (IMAGE_EXT_RE.test(icon)) return 'image'
  // 桌面端不渲染 emoji，统一回退字母头像
  return 'letter'
})

/** 图片 icon 的 asset protocol URL */
const imageSrc = computed(() => {
  const filePath = `${props.extensionPath}/${props.icon!.trim()}`.replace(/\\/g, '/')
  return convertFileSrc(filePath)
})

/** 内联 SVG 消毒：移除 script/foreignObject、事件属性与脚本 URL */
const sanitizedSvg = computed(() => {
  const raw = props.icon!.trim()
  return raw
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<foreignObject[\s\S]*?<\/foreignObject>/gi, '')
    .replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '')
    .replace(/(href|xlink:href)\s*=\s*(["']?)\s*javascript:[^"'>\s]*\2/gi, '')
})

const sizeClass = computed(() =>
  props.size === 'lg' ? 'w-14 h-14 rounded-2xl' : 'w-10 h-10 rounded-xl'
)
</script>

<style scoped>
.icon-tile {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.svg-tile :deep(svg) {
  width: 100%;
  height: 100%;
}
</style>
