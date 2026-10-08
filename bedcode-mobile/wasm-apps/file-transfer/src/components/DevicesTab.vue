<script setup lang="ts">
/**
 * DevicesTab — 附近设备 tab
 *
 * 与旧版功能等价（PeerDevicesSheet 的内容改为一等 tab，不再收进 bottom sheet）：
 * 已连接 / 连接中 / 未连接三态可区分，拨号失败（拒绝/不可达）行内呈现，
 * 已连接可断开、可设为当前活跃对端，无传输能力节点可见但不可连接。
 * 头部提供「探索发现」重扫，尾部统一给出信任提示。
 * 纯展示：行状态由 usePeerDevices 派生的 rows 传入，操作以事件上抛。
 */
import type { DeviceRow } from '../composables/deviceState'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  rows: DeviceRow[]
  /** 探索发现进行中（按钮转 spinner、防重复点击） */
  scanning: boolean
  t: Translate
}>()

const emit = defineEmits<{
  (e: 'connect', nodeId: string): void
  (e: 'disconnect', nodeId: string): void
  (e: 'set-active', nodeId: string): void
  (e: 'scan'): void
}>()

const t = props.t

/** 短指纹（前 8 位） */
function shortFingerprint(nodeId: string): string {
  return nodeId.slice(0, 8)
}

/** 状态点样式：绿=已连接、琥珀=握手中、降饱和绿=在线未连接、灰=离线/无能力 */
function dotClass(row: DeviceRow): string {
  if (row.status === 'connected') return 'fv2-dev-dot--connected'
  if (row.status === 'connecting') return 'fv2-dev-dot--connecting'
  if (row.dialError === 'unreachable') return 'fv2-dev-dot--offline'
  if (!row.fileTransfer) return 'fv2-dev-dot--nocap'
  return 'fv2-dev-dot--reachable'
}

/** 行内元信息（三态 + 地址；无能力节点优先给出能力标注 + IP 便于诊断） */
function metaText(row: DeviceRow): string {
  if (!row.fileTransfer) {
    return row.addr ? `${t('transfer.devices.capNone')} · ${row.addr}` : t('transfer.devices.capNone')
  }
  switch (row.status) {
    case 'connected':
      return row.addr ? `${t('transfer.devices.connected')} · ${row.addr}` : t('transfer.devices.connected')
    case 'connecting':
      return t('transfer.devices.connecting')
    default:
      if (row.recent) {
        return row.addr ? `${t('transfer.devices.recentSeen')} · ${row.addr}` : t('transfer.devices.recentSeen')
      }
      return row.addr ? `${t('transfer.devices.online')} · ${row.addr}` : t('transfer.devices.online')
  }
}
</script>

<template>
  <div class="flex-1 min-h-0 flex flex-col">
    <!-- 标题行：标题 + 副标题 + 行尾探索发现按钮（空态时隐藏，避免与空态内标题/CTA 重复） -->
    <div v-if="rows.length > 0" class="flex-shrink-0 flex items-center gap-2 px-4 pt-1 pb-2">
      <h3 class="page-title flex-shrink-0" style="font-size: var(--font-size-lg)">
        {{ t('transfer.devices.title') }}</h3>
      <span class="flex-1 min-w-0 truncate" style="font-size: var(--font-size-xs); color: var(--mobile-text-muted)">
        {{ t('transfer.v2.devices.subtitle') }}</span>
      <button class="fv2-btn-tint flex-shrink-0" :disabled="scanning" @click="emit('scan')">
        <svg
          v-if="scanning"
          class="w-4 h-4 flex-shrink-0"
          style="animation: fv2-spin 0.8s linear infinite"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        <svg v-else class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        {{ scanning ? t('transfer.devices.scanning') : t('transfer.devices.scan') }}
      </button>
    </div>

    <!-- 列表滚动区 -->
    <div class="flex-1 min-h-0 overflow-y-auto overscroll-behavior-none px-4 pb-2">
      <!-- 空态：发现缓存无节点 -->
      <div v-if="rows.length === 0" class="h-full flex flex-col">
        <div class="fv2-empty">
          <div class="fv2-empty-icon">
            <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M4.72 4.39a9.5 9.5 0 0114.56 0M7.82 5.52a6.5 6.5 0 018.36 0M10.8 7.21a3.5 3.5 0 012.4 0M12 20h.01"
              />
            </svg>
          </div>
          <p class="fv2-empty-title">{{ t('transfer.devices.title') }}</p>
          <p class="fv2-empty-hint">{{ t('transfer.devices.empty') }}</p>
          <button class="fv2-empty-action fv2-btn-primary" @click="emit('scan')">
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            {{ t('transfer.devices.scan') }}
          </button>
        </div>
      </div>

      <template v-else>
        <div v-for="row in rows" :key="row.nodeId" class="group-card fv2-device">
          <div class="fv2-device-body">
            <!-- 状态点 -->
            <span class="fv2-dev-dot flex-shrink-0" :class="dotClass(row)"></span>

            <div class="flex-1 min-w-0">
              <!-- 设备名 + 短指纹 -->
              <div class="flex items-center gap-1.5 min-w-0">
                <span class="fv2-device-name truncate">{{ row.deviceName }}</span>
                <span class="flex-shrink-0" style="font-size: var(--font-size-xs); color: var(--mobile-text-disabled)">
                  {{ shortFingerprint(row.nodeId) }}
                </span>
              </div>
              <p class="fv2-device-meta truncate">{{ metaText(row) }}</p>
              <!-- 拨号终态反馈：拒绝 / 不可达 -->
              <p v-if="row.dialError" class="fv2-device-error">
                {{ row.dialError === 'denied' ? t('transfer.devices.denied') : t('transfer.devices.unreachable') }}
              </p>
            </div>

            <!-- 活跃 / 无能力 标签 -->
            <span v-if="row.isActive" class="ft-chip ft-color-active flex-shrink-0">
              {{ t('transfer.devices.activeCurrent') }}
            </span>
          </div>

          <!-- 行操作 -->
          <div class="fv2-device-actions">
            <button
              v-if="row.status === 'connected' && !row.isActive"
              class="fv2-btn-neutral"
              @click="emit('set-active', row.nodeId)"
            >
              {{ t('transfer.devices.setActive') }}
            </button>
            <button
              v-if="row.status === 'connected'"
              class="fv2-btn-neutral fv2-btn-neutral--danger"
              @click="emit('disconnect', row.nodeId)"
            >
              {{ t('transfer.devices.disconnect') }}
            </button>
            <span v-else-if="row.status === 'connecting'" class="flex-1 flex items-center justify-center fv2-device-meta">
              {{ t('transfer.devices.connecting') }}
            </span>
            <button
              v-else
              class="fv2-btn-primary"
              :disabled="!row.fileTransfer"
              @click="emit('connect', row.nodeId)"
            >
              {{ t('transfer.devices.connect') }}
            </button>
          </div>
        </div>

        <!-- 信任提示（列表尾部一次性说明） -->
        <p class="fv2-device-meta px-1 pt-1 pb-2" style="line-height: 1.5">
          {{ t('transfer.devices.trustHint') }}
        </p>
      </template>
    </div>
  </div>
</template>
