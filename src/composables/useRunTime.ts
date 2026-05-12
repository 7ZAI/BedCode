import { ref, onMounted, onUnmounted, type Ref, type ComputedRef } from 'vue'

/**
 * 运行时间 Composable
 *
 * 实时计算并显示会话运行时间，每秒更新一次
 */
export function useRunTime(
  startTime: () => string | undefined,
  isRunning: Ref<boolean> | ComputedRef<boolean> | (() => boolean)
) {
  const runTime = ref('')
  let intervalId: ReturnType<typeof setInterval> | null = null

  function updateRunTime() {
    const start = startTime()
    if (!start) {
      runTime.value = '--'
      return
    }

    const diff = Math.floor((Date.now() - new Date(start).getTime()) / 1000)

    if (diff < 60) {
      runTime.value = `${diff}秒`
    } else if (diff < 3600) {
      const minutes = Math.floor(diff / 60)
      const seconds = diff % 60
      runTime.value = `${minutes}分${seconds}秒`
    } else {
      const hours = Math.floor(diff / 3600)
      const minutes = Math.floor((diff % 3600) / 60)
      runTime.value = `${hours}小时${minutes}分`
    }
  }

  function startTimer() {
    if (intervalId) return
    updateRunTime()
    intervalId = setInterval(updateRunTime, 1000)
  }

  function stopTimer() {
    if (intervalId) {
      clearInterval(intervalId)
      intervalId = null
    }
  }

  onMounted(() => {
    // 支持 Ref、ComputedRef 或普通函数
    const running = 'value' in isRunning ? (isRunning as Ref<boolean>).value : (isRunning as () => boolean)()
    if (running) {
      startTimer()
    } else {
      runTime.value = '--'
    }
  })

  onUnmounted(() => {
    stopTimer()
  })

  return {
    runTime,
  }
}