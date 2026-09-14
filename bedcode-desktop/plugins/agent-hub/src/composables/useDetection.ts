/**
 * Agent Hub 探测状态编排（票据 02）
 *
 * - 挂载时拉取持久化状态（host-storage），无状态则自动触发首轮探测
 * - guest 每完成一个探测项即 emit `plugin:agent-hub:detection` 全量状态，
 *   本 composable 订阅并覆盖本地状态；detecting 由状态内容派生
 *  （事件名与 guest emit 端一致：插件事件统一 `plugin:<插件短名>:<事件>`）
 * - 乱序免疫 + 超时兜底：guest 推送带单调递增 seq，只接受最新全量，防
 *   中间态（某探测项仍 detecting）覆盖最终态；detecting 超过阈值未收敛
 *   则强制复位并拉 storage 权威态——杜绝事件丢失/卡住时 UI 永久"检测中"
 *   （实测复现：storage 已 ok，界面因事件乱序/丢失卡 detecting）
 */
import { ref, onUnmounted } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState } from '../types'

/** 探测收敛兜底：宿主进程超时 20s（detect.rs TIMEOUT_MS），留余量后强制复位 */
const DETECT_SETTLE_TIMEOUT_MS = 25_000

export function useDetection(context: PluginContext) {
  const state = ref<AgentHubState | null>(null)
  const detecting = ref(false)
  /** 已接受的最新事件 seq（0 = 尚未收到任何带 seq 的事件） */
  let lastSeq = 0
  let settleTimer: ReturnType<typeof setTimeout> | undefined

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

  /**
   * 兜底定时器：detecting 持续超过阈值（探测卡住 / 最终态事件丢失）时
   * 强制复位并拉 storage 权威态，杜绝 UI 永久"检测中"
   */
  function armSettleTimeout() {
    clearTimeout(settleTimer)
    settleTimer = setTimeout(() => {
      if (!detecting.value) return
      console.warn('[Agent Hub] detect did not settle within timeout, forcing refresh')
      detecting.value = false
      void refresh()
    }, DETECT_SETTLE_TIMEOUT_MS)
  }

  function clearSettleTimeout() {
    clearTimeout(settleTimer)
    settleTimer = undefined
  }

  /** 触发探测：后续状态经事件推送回流 */
  async function detect() {
    if (detecting.value) return
    detecting.value = true
    armSettleTimeout()
    try {
      await context.commands.execute('agent-hub.detect', {})
    } catch (e) {
      console.error('[Agent Hub] detect failed', e)
      detecting.value = false
      clearSettleTimeout()
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
    // seq 过滤：探测期间多次推送全量状态，乱序到达的旧事件不得覆盖最新
    // 全量（否则 UI 卡在中间态的 detecting，而 storage 已是最终态）
    const seq = payload?.seq ?? 0
    if (seq > 0 && seq < lastSeq) return
    lastSeq = seq

    state.value = payload
    const kinds = Object.values(payload?.clis ?? {})
    const stillDetecting =
      payload?.envStatus === 'detecting' || kinds.some((c) => c.status === 'detecting')
    detecting.value = stillDetecting
    if (stillDetecting) armSettleTimeout()
    else clearSettleTimeout()
  })

  onUnmounted(() => {
    subscription.dispose()
    clearSettleTimeout()
  })

  return { state, detecting, refresh, detect, requestAuth }
}
