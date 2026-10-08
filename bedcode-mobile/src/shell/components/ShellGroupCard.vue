<template>
  <div
    class="overflow-hidden rounded-[12px] border border-[var(--mobile-group-border)] bg-[var(--mobile-bg-card)] divide-y divide-[var(--mobile-group-divider)]"
    :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
  >
    <slot />
  </div>
</template>

<script setup lang="ts">
/**
 * 分组卡片
 *
 * 只提供容器形状（圆角/描边/分隔线），不含外边距——外边距由各屏的
 * 内容区统一给，避免卡片自带间距后在窄屏上左右不对称。
 */
</script>
