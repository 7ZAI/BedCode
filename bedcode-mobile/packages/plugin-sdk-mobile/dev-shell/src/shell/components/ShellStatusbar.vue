<template>
  <div
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
 * 与宿主 `ShellStatusbar.vue` 同款视觉（时钟 + 信号 + 电量），唯一差异是不做平台判定：
 * 宿主那份经 `usePlatform()` 在桌面预览时整条不渲染（桌面没有系统状态栏，画上去像 bug），
 * 而 dev-shell 的舞台本身就是手机框 / 手机视口，恒有状态栏——保留判定只会让预览与真机不一致。
 *
 * 高度固定 24px：状态条不参与滚动，也不承担交互，避免抢占内容区。
 */
import { onUnmounted, ref } from 'vue'

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