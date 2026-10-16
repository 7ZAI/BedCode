<template>
  <section class="acc rounded-xl border overflow-hidden bg-[var(--mobile-bg-secondary)] border-[var(--mobile-border)]" :class="{ collapsed: !open }">
    <!-- 头部：点击切换折叠 -->
    <button class="w-full flex items-center gap-3 px-4 py-3 text-left active:opacity-80 transition-opacity" @click="open = !open">
      <span v-if="emoji" class="w-5 h-5 flex items-center justify-center text-xs flex-shrink-0">{{ emoji }}</span>
      <span class="flex-1 text-sm font-medium text-[var(--mobile-text-primary)]">{{ title }}</span>
      <span
        v-if="badge !== undefined"
        class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium bg-[var(--mobile-bg-elevated)] text-[var(--mobile-text-secondary)]"
      >
        {{ badge }}
      </span>
      <svg class="chevron w-4 h-4 text-[var(--mobile-text-disabled)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <!-- 内容：grid-template-rows 过渡实现平滑折叠 -->
    <div class="acc-body">
      <div class="overflow-hidden">
        <slot />
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * CollapseSection — 可折叠信息区域
 *
 * 迁移自旧公共组件 `src/components/CollapseSection.vue`（壳内自足化）：
 * 标题行（emoji + 标题 + 计数徽章 + 箭头），内容走默认插槽，高度过渡平滑。
 * 折叠用 `grid-template-rows: 1fr → 0fr`（非 height 动画）以避免重排抖动。
 */
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    title: string
    /** 标题前 emoji 图标 */
    emoji?: string
    /** 标题右侧计数/摘要徽章 */
    badge?: string | number
    defaultOpen?: boolean
  }>(),
  { emoji: '', badge: undefined, defaultOpen: true }
)

const open = ref(props.defaultOpen)
</script>

<style scoped>
.acc .acc-body {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.25s ease;
}

.acc.collapsed .acc-body {
  grid-template-rows: 0fr;
}

.acc .chevron {
  transition: transform 0.25s ease;
}

.acc.collapsed .chevron {
  transform: rotate(-90deg);
}
</style>
