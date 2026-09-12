/**
 * Agent Hub 探测状态编排（票据 02）
 *
 * - 挂载时拉取持久化状态（host-storage），无状态则自动触发首轮探测
 * - guest 每完成一个探测项即 emit `plugin:agent-hub:detection` 全量状态，
 *   本 composable 订阅并覆盖本地状态；detecting 由状态内容派生
 *  （事件名与 guest emit 端一致：插件事件统一 `plugin:<插件短名>:<事件>`）
 */
import { ref, onUnmounted } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState } from '../types'

export function useDetection(context: PluginContext) {
  const state = ref<AgentHubState | null>(null)
  const detecting = ref(false)

  /** 拉取持久化状态；从未探测过则自动触发首轮探测 */
  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-state', {})
      state.value = (data?.state ?? null) as AgentHubState | null
      if (!state.value) {
        await detect()
      }
    } catch (e) {
      console.error('[Agent Hub] get-state failed', e)
    }
  }

  /** 触发探测：后续状态经事件推送回流 */
  async function detect() {
    if (detecting.value) return
    detecting.value = true
    try {
      await context.commands.execute('agent-hub.detect', {})
    } catch (e) {
      console.error('[Agent Hub] detect failed', e)
      detecting.value = false
    }
  }

  /** 目录授权：guest 侧一次批量弹窗，结果随状态事件回流 */
  async function requestAuth() {
    try {
      await context.commands.execute('agent-hub.request-auth', {})
    } catch (e) {
      console.error('[Agent Hub] request-auth failed', e)
    }
  }

  const subscription = context.events.on('plugin:agent-hub:detection', (payload: AgentHubState) => {
    state.value = payload
    const kinds = Object.values(payload?.clis ?? {})
    detecting.value =
      payload?.envStatus === 'detecting' || kinds.some((c) => c.status === 'detecting')
  })

  onUnmounted(() => subscription.dispose())

  return { state, detecting, refresh, detect, requestAuth }
}
