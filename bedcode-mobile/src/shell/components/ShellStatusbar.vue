<template>
  <div
    v-if="platformInfo.isMobile && showOwnStatusbar"
    class="flex items-center justify-between px-4 h-6 flex-shrink-0 text-[var(--font-size-xs)]"
    :style="{ color: 'var(--mobile-text-secondary)' }"
    aria-hidden="true"
  >
    <span class="tabular-nums">{{ currentTime }}</span>
    <span class="flex items-center gap-1">
      <!-- 信号 -->
      <svg width="17" height="12" viewBox="0 0 17 12" fill="currentColor">
        <rect x="0" y="7.5" width="3" height="4.5" rx="1" />
        <rect x="4.6" y="5" width="3" height="7" rx="1" />
        <rect x="9.2" y="2.5" width="3" height="9.5" rx="1" />
        <rect x="13.8" y="0" width="3" height="12" rx="1" opacity=".35" />
      </svg>
      <!-- 电量 -->
      <svg width="25" height="12" viewBox="0 0 25 12" fill="none" stroke="currentColor">
        <rect x=".6" y=".6" width="20" height="10.8" rx="3.2" stroke-width="1.2" />
        <rect x="2.6" y="2.6" width="13" height="6.8" rx="1.6" fill="currentColor" stroke="none" />
        <path d="M23 4v4" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    </span>
  </div>
</template>

<script setup lang="ts">
/**
 * 壳内状态条
 *
 * 只在移动端**且系统状态栏不在屏幕上**时渲染（桌面预览没有系统状态栏，画上去反而像 bug）。
 * 高度固定 24px：状态条不参与滚动，也不承担交互，避免抢内容区。
 *
 * 为什么还要看安全区（2026-10-14 修正）：Android 默认显示系统状态栏，壳又画了一条自己的，
 * 实机就出现**上下两条时间/信号/电量**（实测截图：4:23 与 04:23 堆叠）。壳不做沉浸式全屏，
 * 因此判据是「顶部安全区 > 0」= 系统状态栏占位中，此时壳不再重复画一条。
 */
import { computed, inject, onUnmounted, ref, type Ref } from 'vue'
import { usePlatform } from '../composables/usePlatform'

const { platformInfo } = usePlatform()

/** 顶部安全区（App.vue provide；缺省 0 = 无系统状态栏，壳自画） */
const safeArea = inject<Ref<{ top: number }>>('safeArea', ref({ top: 0 }))
const showOwnStatusbar = computed(() => (safeArea.value.top || 0) === 0)

const currentTime = ref('')

function updateTime(): void {
  const now = new Date()
  currentTime.value = `${now.getHours().toString().padStart(2, '0')}:${now
    .getMinutes()
    .toString()
    .padStart(2, '0')}`
}

updateTime()
const timer = setInterval(updateTime, 30_000)
onUnmounted(() => clearInterval(timer))
</script>
