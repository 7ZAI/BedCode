import { ref, onMounted, onUnmounted, type Ref, type ComputedRef } from 'vue'
import { getI18n } from '@binblink/bedcode-plugin-sdk-mobile'

/**
 * 运行时间 composable（自旧宿主 `src/composables/useRunTime.ts` 迁入插件）
 *
 * 实时计算并显示会话运行时间，每秒更新一次；结束态显示 `--`。
 * 文案取宿主 `common.time.*`（与旧实现同键同源：`justNow` 系列是跨域共用文案，
 * 插件侧不另立一份）。
 */
export function useRunTime(
  startTime: () => string | undefined,
  isRunning: Ref<boolean> | ComputedRef<boolean> | (() => boolean),
) {
  const runTime = ref('')
  let intervalId: ReturnType<typeof setInterval> | null = null

  /** 宿主共享 i18n 的全局取文案（不带插件前缀：`common.*` 是宿主持有键） */
  function t(key: string, params?: Record<string, unknown>): string {
    const i18n: any = getI18n()
    if (!i18n?.global?.t) return key
    return i18n.global.t(key, params)
  }

  function updateRunTime() {
    const start = startTime()
    if (!start) {
      runTime.value = '--'
      return
    }

    const diff = Math.floor((Date.now() - new Date(start).getTime()) / 1000)

    if (diff < 60) {
      runTime.value = t('common.time.secondsAgo', { n: diff })
    } else if (diff < 3600) {
      const minutes = Math.floor(diff / 60)
      const seconds = diff % 60
      runTime.value = t('common.time.minutesSecondsAgo', { m: minutes, s: seconds })
    } else {
      const hours = Math.floor(diff / 3600)
      const minutes = Math.floor((diff % 3600) / 60)
      runTime.value = t('common.time.hoursMinutesAgo', { h: hours, m: minutes })
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
    const running =
      'value' in isRunning ? (isRunning as Ref<boolean>).value : (isRunning as () => boolean)()
    if (running) {
      startTimer()
    } else {
      runTime.value = '--'
    }
  })

  onUnmounted(() => {
    stopTimer()
  })

  return { runTime }
}
