import { ref, watch } from 'vue'
import { useAndroidFeatures } from './useAndroidFeatures'

/**
 * 前后台状态监听
 *
 * 监听应用进入前台/后台的状态变化
 * 用于后台运行时触发通知等场景
 *
 * 注意：内部委托给 useAndroidFeatures 管理 isInBackground 状态
 * 本模块只添加 wasInBackground 功能
 */
export function useBackgroundMonitor() {
  const { isInBackground } = useAndroidFeatures()

  // 用于检测刚从后台恢复 - 在恢复后立即设为 true，消费者处理后需清除
  const wasInBackground = ref(false)

  // 记录上一次的状态，用于检测变化
  let previousIsInBackground = isInBackground.value

  // 监听 isInBackground 的变化
  watch(isInBackground, (newValue) => {
    // 从后台恢复到前台
    if (!newValue && previousIsInBackground) {
      wasInBackground.value = true
    }
    previousIsInBackground = newValue
  })

  /**
   * 清除 wasInBackground 标志
   * 消费者在处理完"刚从后台恢复"的事件后应该调用此方法
   */
  function clearWasInBackground() {
    wasInBackground.value = false
  }

  return {
    isInBackground,
    wasInBackground,
    clearWasInBackground,
  }
}