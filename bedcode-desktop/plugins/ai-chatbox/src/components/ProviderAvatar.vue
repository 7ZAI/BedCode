<template>
  <!-- 预设品牌图标；无 presetId 时按名称首字符 + 哈希取色渲染圆形头像 -->
  <span
    class="flex-shrink-0 rounded-full flex items-center justify-center overflow-hidden select-none"
    :style="avatarStyle"
  >
    <!-- 品牌图标内联渲染：fill="currentColor" 需继承主题文字色（<img> 加载的隔离文档
         使 currentColor 恒为黑色，深色主题下不可见）；静态资源无注入风险 -->
    <span v-if="icon" v-html="icon" class="w-full h-full text-[var(--text-primary)] brand-icon" aria-hidden="true"></span>
    <span v-else-if="initial" class="text-white font-medium">{{ initial }}</span>
    <!-- 名称为空时的极简兜底：通用 bot 图标 -->
    <svg v-else class="w-1/2 h-1/2" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d="M9 17v2a2 2 0 002 2h2a2 2 0 002-2v-2M9 4h6M5 12h14a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4a1 1 0 011-1z" />
    </svg>
  </span>
</template>

<script setup lang="ts">
/**
 * ProviderAvatar — 供应商头像
 *
 * 有 presetId → 内置品牌 SVG；否则圆形底色 + 名称首字符（哈希取色，确定性）。
 * 名称为空时渲染通用 bot 图标兜底，避免空圆点。
 */
import { computed } from 'vue'
import { resolveProviderIcon, providerAvatarColor } from '../utils/providerIcons'

const props = withDefaults(
  defineProps<{
    presetId?: string
    name: string
    /** 头像边长（px；列表行/模板卡片共用） */
    size?: number
  }>(),
  { size: 36 },
)

const icon = computed(() => resolveProviderIcon(props.presetId))
const avatarColor = computed(() => providerAvatarColor(props.name))
/** 首字符按码点截取，避免代理对（emoji）被拆成半个 */
const initial = computed(() => Array.from(props.name.trim())[0] || '')

/** 尺寸 + 圆形底色（首字母头像才需要背景色） */
const avatarStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  fontSize: `${Math.round(props.size * 0.42)}px`,
  lineHeight: '1',
  ...(icon.value ? {} : { backgroundColor: avatarColor.value }),
}))
</script>

<style scoped>
/* 内联品牌 SVG 铺满头像容器（源文件仅 viewBox + path，无尺寸属性） */
.brand-icon :deep(svg) {
  width: 100%;
  height: 100%;
}
</style>
