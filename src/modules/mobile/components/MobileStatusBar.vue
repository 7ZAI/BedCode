<template>
  <div
    v-if="platformInfo.isMobile && showStatusBar"
    class="mobile-status-bar"
  >
    <div class="status-bar-content">
      <!-- 时间 -->
      <span class="time">{{ currentTime }}</span>

      <!-- 信号/电池等状态图标 -->
      <div class="status-icons">
        <!-- 电池图标 -->
        <svg class="w-4 h-4 text-[var(--mobile-accent)]" viewBox="0 0 24 24" fill="currentColor">
          <path d="M15.67 4H14V2h-4v2H8.33C7.6 4 7 4.6 7 5.33v15.33C7 21.4 7.6 22 8.33 22h7.33c.74 0 1.34-.6 1.34-1.33V5.33C17 4.6 16.4 4 15.67 4z"/>
        </svg>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { usePlatform } from '@/modules/shared/composables/usePlatform'

const { platformInfo } = usePlatform()

const showStatusBar = ref(true)
const currentTime = ref('')

function updateTime() {
  const now = new Date()
  const hours = now.getHours().toString().padStart(2, '0')
  const minutes = now.getMinutes().toString().padStart(2, '0')
  currentTime.value = `${hours}:${minutes}`
}

let timeInterval: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  updateTime()
  timeInterval = setInterval(updateTime, 1000)
})

onUnmounted(() => {
  if (timeInterval) {
    clearInterval(timeInterval)
  }
})
</script>

<style scoped>
.mobile-status-bar {
  /* 使用 sticky 定位替代 fixed，融入文档流避免遮挡内容 */
  position: sticky;
  top: 0;
  left: 0;
  right: 0;
  height: 24px;
  background: var(--mobile-bg-primary);
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 0 16px;
  font-size: 12px;
  font-weight: 500;
  color: var(--mobile-accent);
}

.status-bar-content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.time {
  min-width: 40px;
  text-align: right;
}

.status-icons {
  display: flex;
  align-items: center;
  gap: 4px;
}
</style>