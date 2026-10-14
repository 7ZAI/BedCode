<template>
  <section class="shell-card card-frame">
    <h3 class="shell-card-title">
      <ShellIcon :name="icon" :size="14" />
      <span>{{ title }}</span>
    </h3>
    <p v-if="desc" class="shell-card-desc">{{ desc }}</p>
    <slot />
  </section>
</template>

<script setup lang="ts">
/**
 * ShellCard — 壳内统一卡片容器（系统页 / 应用详情通用）
 * 标准 --radius-card 圆角 + 分组标题 + 描述 + 内容插槽。
 */
import ShellIcon from './ShellIcon.vue'

withDefaults(
  defineProps<{
    title: string
    desc?: string
    icon?: string
  }>(),
  { icon: '', desc: '' },
)
</script>

<style scoped>
.shell-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-card);
  padding: 12px 14px;
}
.shell-card-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: calc(13px * var(--ui-scale));
  font-weight: 600;
  color: var(--text-primary);
}
.shell-card-desc {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
  line-height: 1.55;
  margin-top: -4px;
}
</style>
