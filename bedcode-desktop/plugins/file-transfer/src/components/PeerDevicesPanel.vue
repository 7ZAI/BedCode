<script setup lang="ts">
/**
 * PeerDevicesPanel — 附近设备面板（自绘，替代旧「在线设备切换下拉」）
 *
 * 列出全部发现的 BedCode 节点：已连接 / 连接中 / 未连接三态可区分，
 * 拨号失败（拒绝/不可达）以行内错误文案呈现；已连接设备可断开、
 * 可设为当前活跃对端；无传输能力节点可见但不可连接。
 * 纯展示组件：状态由 usePeerDevices 派生的 rows 传入，操作以事件上抛。
 */
import { inject } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-desktop'
import type { DeviceRow } from '../composables/deviceState'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const props = defineProps<{
  rows: DeviceRow[]
  /** 探索发现进行中（扫描按钮转 spinner、防重复点击） */
  scanning?: boolean
}>()

const emit = defineEmits<{
  connect: [nodeId: string]
  disconnect: [nodeId: string]
  setActive: [nodeId: string]
  /** 探索发现：重新扫描同网节点（父组件负责调 query-peer 并维护 scanning 态） */
  scan: []
}>()

/** 短指纹（前 8 位）展示 */
function shortFingerprint(nodeId: string): string {
  return nodeId.slice(0, 8)
}

/** 状态点样式：绿=已连接、琥珀呼吸=握手中、灰=未连接/失败 */
function dotClass(row: DeviceRow): string {
  if (row.status === 'connected') return 'ft-dot--online'
  if (row.status === 'connecting') return 'ft-dot--connecting'
  // 不可达终态：降为离线灰，消除「在线」文案与错误提示的矛盾（评审 P2）
  if (row.dialError === 'unreachable') return 'ft-dot--offline'
  // 在线未连接：降饱和绿，与实心绿（已连接）区分（评审 P1：状态点语义）
  return 'ft-dot--reachable'
}
</script>

<template>
  <div class="ft-dev-panel">
    <div class="ft-dev-head">
      <span class="ft-dev-title">{{ t('transfer.devices.title') }}</span>
      <span class="ft-dev-subtitle">{{ t('transfer.devices.subtitle') }}</span>
      <!-- 探索发现：主动重新扫描同网节点；扫描中转 spinner 防重复发起 -->
      <button
        class="ft-text-btn ft-dev-scan-btn"
        :disabled="props.scanning"
        :title="t('transfer.devices.scan')"
        @click="emit('scan')"
      >
        <!-- 扫描中：旋转圆环占位（GPU 合成 transform） -->
        <svg
          v-if="props.scanning"
          class="ft-dev-scan-spin"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
          />
        </svg>
        <svg v-else class="ft-dev-scan-ico" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
          />
        </svg>
        {{ props.scanning ? t('transfer.devices.scanning') : t('transfer.devices.scan') }}
      </button>
    </div>

    <!-- 空态：发现缓存无节点 -->
    <div v-if="rows.length === 0" class="ft-dev-empty">
      {{ t('transfer.devices.empty') }}
    </div>

    <!-- 设备行列表 -->
    <div v-else class="ft-dev-list">
      <div
        v-for="row in rows"
        :key="row.nodeId"
        class="ft-dev-row"
        :class="{ 'ft-dev-row--active': row.isActive }"
      >
        <div class="ft-dev-info">
          <div class="ft-dev-name-line">
            <span class="ft-dot" :class="dotClass(row)"></span>
            <span class="ft-dev-name">{{ row.deviceName }}</span>
            <span class="ft-dev-fp">{{ shortFingerprint(row.nodeId) }}</span>
            <span
              v-if="row.isActive"
              class="ft-dev-tag ft-dev-tag--active"
            >
              {{ t('transfer.devices.activeCurrent') }}
            </span>
            <span
              v-else-if="!row.fileTransfer"
              class="ft-dev-tag"
            >
              {{ t('transfer.devices.capNone') }}
            </span>
          </div>
          <div class="ft-dev-meta">
            <template v-if="row.status === 'connected'">
              {{ t('transfer.devices.connected') }}<template v-if="row.addr"> · {{ row.addr }}</template>
            </template>
            <template v-else-if="row.status === 'connecting'">
              {{ t('transfer.devices.connecting') }}
            </template>
            <template v-else-if="row.recent">
              {{ t('transfer.devices.recentSeen') }}<template v-if="row.addr"> · {{ row.addr }}</template>
            </template>
            <template v-else>
              {{ t('transfer.devices.online') }}<template v-if="row.addr"> · {{ row.addr }}</template>
            </template>
          </div>
          <!-- 拨号终态反馈：拒绝 / 不可达（行内错误，非全局 toast） -->
          <div v-if="row.dialError" class="ft-dev-error">
            {{
              row.dialError === 'denied'
                ? t('transfer.devices.denied')
                : t('transfer.devices.unreachable')
            }}
          </div>
        </div>

        <div class="ft-dev-actions">
          <button
            v-if="row.status === 'connected' && !row.isActive"
            class="ft-text-btn"
            @click="emit('setActive', row.nodeId)"
          >
            {{ t('transfer.devices.setActive') }}
          </button>
          <button
            v-if="row.status === 'connected'"
            class="ft-text-btn ft-text-btn--danger"
            @click="emit('disconnect', row.nodeId)"
          >
            {{ t('transfer.devices.disconnect') }}
          </button>
          <span v-else-if="row.status === 'connecting'" class="ft-dev-connecting">
            {{ t('transfer.devices.connecting') }}
          </span>
          <button
            v-else
            class="ft-text-btn ft-text-btn--primary"
            :disabled="!row.fileTransfer"
            @click="emit('connect', row.nodeId)"
          >
            {{ t('transfer.devices.connect') }}
          </button>
        </div>
      </div>
    </div>

    <div class="ft-dev-hint">{{ t('transfer.devices.trustHint') }}</div>
  </div>
</template>
