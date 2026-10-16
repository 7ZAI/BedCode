<template>
  <span
    class="inline-flex items-center justify-center flex-shrink-0 overflow-hidden"
    :class="toneClass"
    :style="boxStyle"
    role="img"
    :aria-label="app.name"
  >
    <!-- 图片图标：清单里写的是相对插件目录的路径（icon.svg / icon.png …），
         经 asset protocol 取图；加载失败退回首字母，不留空白格 -->
    <img
      v-if="kind === 'image' && app.iconUrl && !imageFailed"
      :src="app.iconUrl"
      :alt="app.name"
      class="w-full h-full object-cover"
      @error="imageFailed = true"
    />
    <!-- SVG path d（应用自带图标，24×24 视框 stroke 风格） -->
    <svg
      v-else-if="kind === 'path'"
      :width="innerSize"
      :height="innerSize"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path :d="app.icon" />
    </svg>
    <!-- emoji 图标 -->
    <span
      v-else-if="kind === 'emoji'"
      :style="{ fontSize: `${innerSize}px`, lineHeight: '1' }"
      aria-hidden="true"
      >{{ app.icon }}</span
    >
    <!-- 无图标 / 图片加载失败：首字母回退（中文名取首字比取不到拉丁字母更可读） -->
    <span
      v-else
      :style="{ fontSize: `${Math.round(size * 0.42)}px`, fontWeight: 600 }"
      aria-hidden="true"
      >{{ initial }}</span
    >
  </span>
</template>

<script setup lang="ts">
/**
 * 应用图标
 *
 * 四种形态归一：图片路径 / emoji / SVG path d / 首字母回退。判据集中在 utils.iconKindOf，
 * 避免各屏各写一份「这是不是 path」的判断。
 */
import { computed, ref, watch } from 'vue'
import type { ShellApp } from '../types'
import { iconKindOf, initialOf } from '../utils'

const props = withDefaults(
  defineProps<{
    app: ShellApp
    /** 图标盒边长（px） */
    size?: number
    /** 色调：accent（强调底）/ plain（中性底）/ ghost（幽灵框，用于「更多」占位） */
    tone?: 'accent' | 'plain' | 'ghost'
  }>(),
  { size: 44, tone: 'accent' },
)

const kind = computed(() => iconKindOf(props.app.icon))
const initial = computed(() => initialOf(props.app.name))
/**
 * 图形占盒比例：三种形态各自留白不同，同一盒子才显得是一套。
 * SVG 线稿本身有内边距，占 0.55 即视觉接近满格；首字母是实心字形，占 0.42；
 * emoji 是插件清单里自带的品牌位，字面大、视觉重量最重，压到 0.52——
 * 否则彩色 emoji 会在同为 44px 的格子里比旁边克制的线稿图标明显更响。
 */
const innerSize = computed(() => {
  const ratio = kind.value === 'emoji' ? 0.52 : 0.55
  return Math.round(props.size * ratio)
})

/** 图片加载失败标记：插件换图标后重置，避免停在首字母回退 */
const imageFailed = ref(false)
watch(
  () => [props.app.icon, props.app.iconUrl],
  () => {
    imageFailed.value = false
  },
)

/**
 * 作品真正在场（图片图标且未回退）：此时图标盒完全退让——零描边、零底色。
 *
 * 移动端插件图标（plugin.json 的 icon）都是满幅圆角方作品（icon.svg：自带
 * 底色 + rx 18~20% 圆角），盒子再画 1px 描边 + 铺一层比页面更亮的底，就只在
 * 作品四周添一圈「白框」——作品自己的圆角（≈size 的 8px）比盒子的圆角
 * （size 的 11px）小，两者之间的缝里漏出的正是盒底本身。用户实测反馈的
 * 「图标有白色边框」就是这个缝 + 描边。盒子只保留圆角裁切。
 */
const artworkShown = computed(
  () => kind.value === 'image' && !!props.app.iconUrl && !imageFailed.value,
)

/** 圆角随尺寸线性放大并钳制：小盒（列表行）接近方形角，大可盒（预览位）过渡到拟 Windows/Android 大圆角，避免同一图标在 12/16 两档间跳变 */
const borderRadius = computed(() => {
  const r = Math.round(props.size * 0.25)
  return `${Math.min(18, Math.max(10, r))}px`
})

const boxStyle = computed(() => ({
  width: `${props.size}px`,
  height: `${props.size}px`,
  borderRadius: borderRadius.value,
}))

const toneClass = computed(() => {
  // 作品在场时不做任何外框（见 artworkShown 注释）
  if (artworkShown.value) return 'bg-transparent'
  switch (props.tone) {
    case 'plain':
      return 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)]'
    case 'ghost':
      // ghost 的全部视觉身份就是那圈虚线框，描边只属于它一档
      return 'bg-transparent text-[var(--mobile-text-secondary)] border border-dashed border-[var(--mobile-border-hover)]'
    default:
      // accent 底板对四种形态取同一个表面（**不含描边**：移动端主线图标是满幅作品，
      // 描边只会成为作品外的一圈白框；桌面壳同位的 .glyph-box 也没有描边）。
      // 早前 emoji / 图片走中性底、首字母回退走 accent 淡底，同一宫格里出现
      // 深色块与浅灰块两种质感，读起来像两种控件；这里收成一套，格子只由
      // 里面的图形区分，不靠底色。
      return 'bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-primary)]'
  }
})
</script>
