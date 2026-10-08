<template>
  <nav
    class="flex-shrink-0 border-t border-[var(--mobile-group-border)]"
    :style="{ background: 'var(--mobile-nav-bg)', paddingBottom: `${bottomInset}px` }"
    :aria-label="t('shell.nav.home')"
  >
    <div class="flex justify-around">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        class="relative flex flex-col items-center gap-0.5 px-4 pt-2 pb-2 rounded-[12px] transition-colors duration-200 min-h-[var(--mobile-nav-item-height)] min-w-[var(--mobile-touch-target-min)]"
        :class="isActive(tab.id) ? 'text-[var(--mobile-nav-active)]' : ''"
        :style="isActive(tab.id) ? {} : { color: 'var(--mobile-nav-inactive)' }"
        :aria-current="isActive(tab.id) ? 'page' : undefined"
        @click="nav.switchTab(tab.id)"
      >
        <!-- 激活指示条：锚定按钮顶部居中（与既有 MobileNav 同一形态） -->
        <span
          v-if="isActive(tab.id)"
          class="absolute top-0 left-0 right-0 mx-auto w-6 h-[2px] rounded-full"
          :style="{ background: 'var(--mobile-nav-active)' }"
          aria-hidden="true"
        />
        <svg class="flex-shrink-0" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path :d="tab.icon" />
        </svg>
        <span class="text-[var(--font-size-xs)]" :class="isActive(tab.id) ? 'font-semibold' : 'font-medium'">
          {{ tab.label }}
        </span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
/**
 * 壳内底部导航
 *
 * 三个 Tab 都是平台自己的屏（首页 / 应用 / 我的）。应用运行面不显示本导航——
 * 应用自带导航语义，再压一层平台 Tab 会让两个返回层级打架。
 */
import { computed, inject, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellNavigation, type ShellScreenId } from '../composables/useShellNavigation'

interface ShellTab {
  id: ShellScreenId
  label: string
  icon: string
}

const { t } = useI18n()
const nav = useShellNavigation()

const tabs = computed<ShellTab[]>(() => [
  {
    id: 'home',
    label: t('shell.nav.home'),
    icon: 'M3 10.5L12 3l9 7.5 M5 9.5V21h14V9.5 M10 21v-6h4v6',
  },
  {
    id: 'apps',
    label: t('shell.nav.apps'),
    icon: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
  },
  {
    id: 'settings',
    label: t('shell.nav.settings'),
    icon: 'M12 8a4 4 0 100-8 4 4 0 000 8z M4.5 21a7.5 7.5 0 0 1 15 0',
  },
])

function isActive(id: ShellScreenId): boolean {
  return nav.current.value.id === id
}

// 安全区由 App.vue 注入；Android WebView 不支持 env()，只能取 JS 值
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }> | undefined>(
  'safeArea',
  undefined,
)
const bottomInset = computed(() => {
  const value = safeArea?.value
  return value?.navigationBar ?? value?.bottom ?? 0
})
</script>
