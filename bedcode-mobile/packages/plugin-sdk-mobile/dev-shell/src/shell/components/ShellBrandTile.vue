<template>
  <span
    class="inline-flex items-center justify-center flex-shrink-0 rounded-[12px] border border-[var(--mobile-border)]"
    :style="{ width: `${size}px`, height: `${size}px`, background: 'var(--shell-brand-ink)' }"
    role="img"
    :aria-label="BRAND_NAME"
  >
    <svg :width="size * 0.62" :height="size * 0.62" viewBox="0 0 24 24" aria-hidden="true">
      <!-- 品牌核心色：固定琥珀（深底 8.99:1）。不随用户色板变化，也不用于可点击控件 -->
      <path d="M12 3 L18.5 12 L12 21 L5.5 12 Z" fill="var(--shell-brand-ember)" />
      <path d="M12 8 L14.8 12 L12 16 L9.2 12 Z" fill="var(--shell-brand-ink)" />
    </svg>
  </span>
</template>

<script setup lang="ts">
/**
 * 品牌标志底板
 *
 * 品牌层与 UI 层分离（brand-spec-mobile.md §1）：底板恒为品牌深墨，图形恒为琥珀，
 * 两者都不跟 --mobile-accent / data-palette 走。用户把主色换成海蓝时标志仍是琥珀。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 38 })

/** 品牌名是专有名词，不做本地化 */
const BRAND_NAME = 'WasmApp'
</script>
