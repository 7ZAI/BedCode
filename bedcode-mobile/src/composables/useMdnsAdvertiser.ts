/**
 * mDNS Advertiser Composable（退役壳）
 *
 * 2026-10-10 能力域集中化：移动端广播面（`mdns_start_advertise` /
 * `mdns_stop_advertise`）在宿主 Rust 侧已退役，防回接锁
 * `retired_mobile_mdns_advertise_lock.rs` 钉死不得复活；本模块原唯一调用点
 * `useMdnsAdvertiser`（App.vue 广告入口，旧版已注释）无活消费者。
 *
 * 保留空壳：旧前端（回退至 92222e8b6）仍 import 本模块，删除会让重构
 * 波及寄存器面；但广播调用整体移除，isAdvertising 恒 false，接口保持
 * 形状兼容以便未来按需删除。
 */

import { ref, readonly } from 'vue'

// ==================== Global State ====================

const isAdvertising = ref(false)

// ==================== Composable ====================

/**
 * mDNS 广播 composable
 *
 * 全局单例模式。广播面已退役：startAdvertise 直接置位成功
 * （不触后端命令），stopAdvertise 复位，均无网络副作用。
 */
export function useMdnsAdvertiser() {
  async function startAdvertise(_port: number, _deviceName: string) {
    // 广播面已退役（退役锁钉死）：不 invoke mdns_*_advertise，
    // 仅保持状态机形状，无活消费者受影响
    isAdvertising.value = true
  }

  async function stopAdvertise() {
    isAdvertising.value = false
  }

  return {
    isAdvertising: readonly(isAdvertising),
    startAdvertise,
    stopAdvertise,
  }
}