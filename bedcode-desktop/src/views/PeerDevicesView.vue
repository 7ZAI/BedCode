<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：标题 + 手动刷新兜底 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3 min-w-0">
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('peers.devices.title') }}
        </h2>
        <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
          {{ t('peers.devices.subtitle') }}
        </span>
      </div>
      <div class="flex items-center gap-2 flex-shrink-0">
        <button class="wb-btn-ghost" @click="refresh">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99"
            />
          </svg>
          {{ t('common.button.refresh') }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-5">
      <!-- ==================== 空态：发现缓存无节点 ==================== -->
      <div
        v-if="peers.length === 0"
        class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-10 text-center"
      >
        <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
          {{ t('peers.devices.empty') }}
        </p>
      </div>

      <!-- ==================== 设备卡片列表：名称/指纹/能力标记 + 连接操作 ==================== -->
      <div v-else class="space-y-3 max-w-3xl">
        <div
          v-for="peer in peers"
          :key="peer.nodeId"
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3 flex items-center gap-4"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 min-w-0">
              <!-- 在线状态点：发现缓存内即在线，颜色随连接态强调 -->
              <span
                class="w-2 h-2 rounded-full flex-shrink-0 transition-colors duration-200"
                :class="
                  isConnected(peer.nodeId)
                    ? 'bg-green-500'
                    : 'bg-[var(--color-primary)] opacity-70'
                "
              ></span>
              <span
                class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
              >
                {{ peer.deviceName }}
              </span>
              <span class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] flex-shrink-0">
                {{ shortFingerprint(peer.nodeId) }}
              </span>
              <!-- 能力标记：文件传输有无一目了然 -->
              <span
                class="px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium flex-shrink-0"
                :class="
                  peer.fileTransfer
                    ? 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
                    : 'bg-transparent border border-dashed border-[var(--border)] text-[var(--text-tertiary)]'
                "
              >
                {{
                  peer.fileTransfer ? t('peers.devices.capFileTransfer') : t('peers.devices.capNone')
                }}
              </span>
            </div>
            <p class="mt-1 text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] truncate">
              {{ t('peers.devices.online') }} · {{ peer.addr }}
            </p>
            <!-- 拨号终态反馈：拒绝/不可达（已连接态由右侧徽标呈现） -->
            <p
              v-if="dialError(peer.nodeId)"
              class="mt-0.5 text-[calc(11px*var(--ui-scale))] text-red-500 dark:text-red-400"
            >
              {{ dialError(peer.nodeId) === 'denied' ? t('peers.devices.denied') : t('peers.devices.unreachable') }}
            </p>
          </div>

          <div class="flex items-center gap-2 flex-shrink-0">
            <span
              v-if="isConnected(peer.nodeId)"
              class="inline-flex items-center gap-1 px-2 h-7 rounded-md bg-green-500/10 text-green-600 dark:text-green-400 text-[calc(11px*var(--ui-scale))] font-medium"
            >
              {{ t('peers.devices.connected') }}
            </span>
            <span
              v-else-if="isConnecting(peer.nodeId)"
              class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]"
            >
              {{ t('peers.devices.connecting') }}
            </span>
            <!-- 浏览对端共享目录（issue 11）：仅已连接（互信）态可见，只读入口 -->
            <button
              v-if="isConnected(peer.nodeId) && peer.fileTransfer"
              class="wb-btn-ghost"
              @click="browseFiles(peer)"
            >
              {{ t('peers.files.browse') }}
            </button>
            <button
              v-if="isConnected(peer.nodeId)"
              class="wb-btn-ghost"
              @click="disconnect(peer.nodeId)"
            >
              {{ t('peers.devices.disconnect') }}
            </button>
            <button
              v-else-if="!isConnecting(peer.nodeId)"
              class="wb-btn-primary"
              :disabled="!peer.fileTransfer"
              @click="handleConnect(peer)"
            >
              {{ t('peers.devices.connect') }}
            </button>
          </div>
        </div>

        <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 pt-1">
          {{ t('peers.devices.trustHint') }}
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 对等网络设备列表页 — 发现缓存驱动的「看得见」入口（issue 08）
 *
 * 列表随节点上下线自动刷新（宿主 peer-devices-changed 推送，手动刷新仅为
 * 兜底）；具备文件传输能力的节点可发起连接——对端确认后进入已连接态；
 * 无能力节点可见但不可连接。信任管理在 设置 → 可信对端。
 */
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import {
  usePeerDevices,
  type DiscoveredPeer,
} from '@/composables/usePeerDevices'

const { t } = useI18n()
const router = useRouter()
const { peers, connectingIds, connectedIds, dialErrors, start, refresh, connect, disconnect } =
  usePeerDevices()

/** 短指纹（前 8 位）展示 */
function shortFingerprint(nodeId: string): string {
  return nodeId.slice(0, 8)
}

function isConnecting(nodeId: string): boolean {
  return connectingIds.value.has(nodeId)
}

function isConnected(nodeId: string): boolean {
  return connectedIds.value.has(nodeId)
}

function dialError(nodeId: string): string | undefined {
  return dialErrors.value[nodeId]
}

async function handleConnect(peer: DiscoveredPeer): Promise<void> {
  await connect(peer.nodeId)
}

/** 进入对端远端文件页（issue 11；携带设备名供页头展示） */
function browseFiles(peer: DiscoveredPeer): void {
  void router.push({
    name: 'peer-files',
    params: { nodeId: peer.nodeId },
    query: { name: peer.deviceName },
  })
}

onMounted(() => {
  void start()
})
</script>
