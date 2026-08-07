<template>
  <!-- 图片图标：相对插件目录的资源路径（png/jpg/webp/svg 等），经 asset protocol 加载 -->
  <div v-if="kind === 'image'" class="icon-tile overflow-hidden" :class="sizeClass">
    <img v-if="!imgFailed" :src="imageSrc" :alt="name" class="w-full h-full object-cover" @error="imgFailed = true" />
    <LetterAvatar v-else :name="name" :seed="pluginId" :size="size" />
  </div>

  <!-- 内联 SVG：manifest.icon 直接携带 <svg> 标记 -->
  <div v-else-if="kind === 'svg'" class="icon-tile svg-tile" :class="sizeClass" v-html="sanitizedSvg"></div>

  <!-- 原始 SVG path data：Heroicons outline 风格，viewBox=0 0 24 24 -->
  <div v-else-if="kind === 'path'" class="icon-tile svg-tile" :class="sizeClass">
    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="icon" />
    </svg>
  </div>

  <!-- emoji 图标 -->
  <div
    v-else-if="kind === 'emoji'"
    class="icon-tile"
    :class="[sizeClass, size === 'lg' ? 'text-4xl' : 'text-2xl']"
    style="background: linear-gradient(135deg, var(--mobile-accent-secondary), var(--mobile-accent-muted)); border: 1px solid var(--mobile-border-active)"
  >
    {{ icon }}
  </div>

  <!-- 回退：渐变字母头像 -->
  <LetterAvatar v-else :name="name" :seed="pluginId" :size="size" />
</template>

<script setup lang="ts">
/**
 * PluginIcon - 插件图标（深模块）
 *
 * 调用方只传 manifest 信息，四级回退逻辑全部隐藏在内：
 * 1. manifest.icon 为图片路径（png/jpg/webp/gif/svg）→ 经 asset protocol 加载
 * 2. manifest.icon 为内联 <svg> 标记 → 消毒后直接渲染（免图片文件）
 * 3. manifest.icon 为原始 SVG path data（M/m 开头）→ 渲染为内联 <path>
 * 4. manifest.icon 为 emoji/短文本 → 直接渲染
 * 5. 无 icon 或图片加载失败 → 按插件 id 哈希生成渐变字母头像
 */
import { computed, ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import LetterAvatar from './LetterAvatar.vue'

const props = withDefaults(
  defineProps<{
    /** manifest.icon：emoji、内联 <svg> 标记或相对插件目录的图片路径 */
    icon?: string
    /** 插件名（字母头像取首字符） */
    name: string
    /** 插件 id（字母头像渐变配色的哈希种子） */
    pluginId: string
    /** 插件目录路径，图片 icon 相对此目录解析 */
    extensionPath?: string
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

/** SVG path data 判定：Heroicons outline 风格 d 字符串（M/m 开头） */
const SVG_PATH_RE = /^[Mm]\s*[\d.]/

/** 图标类型判定：图片路径 / 内联 SVG / SVG path / emoji / 回退字母头像 */
const kind = computed<'image' | 'svg' | 'path' | 'emoji' | 'letter'>(() => {
  const icon = props.icon?.trim()
  if (!icon) return 'letter'
  if (icon.startsWith('<svg')) return 'svg'
  if (IMAGE_EXT_RE.test(icon)) return 'image'
  if (SVG_PATH_RE.test(icon)) return 'path'
  return 'emoji'
})

/** 图片 icon 的 asset protocol URL */
const imageSrc = computed(() => {
  const filePath = `${props.extensionPath}/${props.icon.trim()}`.replace(/\\/g, '/')
  return convertFileSrc(filePath)
})

/**
 * 内联 SVG 消毒：移除 script/foreignObject、事件属性与脚本 URL。
 * 插件本身已具备 JS 执行能力，此处为纵深防御，防止 icon 字段被第三方 manifest 滥用
 */
const sanitizedSvg = computed(() => {
  const raw = props.icon.trim()
  return raw
    .replace(/<script[\s\S]*?<\/script>/gi, '')
    .replace(/<foreignObject[\s\S]*?<\/foreignObject>/gi, '')
    .replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '')
    .replace(/(href|xlink:href)\s*=\s*(["']?)\s*javascript:[^"'>\s]*\2/gi, '')
})

const sizeClass = computed(() =>
  props.size === 'lg' ? 'w-16 h-16 rounded-2xl' : 'w-12 h-12 rounded-xl'
)
</script>

<style scoped>
.icon-tile {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

/* 内联 SVG 自适应铺满图标容器 */
.svg-tile :deep(svg) {
  width: 100%;
  height: 100%;
}
</style>
