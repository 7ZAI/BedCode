<template>
  <span
    class="inline-flex items-center justify-center flex-shrink-0 rounded-[12px] border border-[var(--mobile-border)]"
    :style="{
      width: `${size}px`,
      height: `${size}px`,
      background: 'linear-gradient(135deg, var(--shell-brand-ink), var(--shell-brand-ink-deep))',
    }"
    role="img"
    :aria-label="BRAND_NAME"
  >
    <svg :width="size * 0.62" :height="size * 0.62" viewBox="0 0 100 100" aria-hidden="true">
      <!-- 标志几何与应用图标逐字同源（src-tauri/icons/icon.svg）：暖炭底 + 白色 `>` 终端
           提示符。translate/scale 把 100×100 的图标坐标系缩进底板（图形约占底板 41%），
           与旧标志在底板中的视觉重量对齐。底色不写死在内联 style 里，走品牌层 token，
           用户换色板时标志不变。 -->
      <g transform="translate(16 16) scale(0.68)" fill="var(--shell-brand-glyph)">
        <path d="M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z" />
        <path d="M 51 60 L 84 62 L 53 65 Z" />
      </g>
    </svg>
  </span>
</template>

<script setup lang="ts">
/**
 * 品牌标志底板
 *
 * 品牌层与 UI 层分离：底板恒为品牌暖炭、图形恒为白，两者都不跟 --mobile-accent /
 * data-palette 走——用户把主色换成海蓝时标志仍是暖炭 + 白。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 38 })

/** 品牌名是专有名词，不做本地化 */
const BRAND_NAME = 'BedCode'
</script>
