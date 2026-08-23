<template>
  <SettingsSubPage :title="$t('peers.devices.title')">
    <div class="px-4 py-4 space-y-3">
      <!-- 空态 -->
      <div v-if="peers.length === 0" class="settings-group py-8 text-center">
        <p class="text-sm text-[var(--mobile-text-muted)]">{{ $t('peers.devices.empty') }}</p>
      </div>

      <!-- 设备卡片：名称/短指纹/地址 + 能力标记 + 连接操作 -->
      <div
        v-for="peer in peers"
        :key="peer.nodeId"
        class="settings-group p-4"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 min-w-0">
              <!-- 在线状态点：发现缓存内即在线，已连接时强调 -->
              <span
                class="w-2 h-2 rounded-full flex-shrink-0 transition-colors duration-200"
                :style="{ background: isConnected(peer.nodeId) ? 'var(--mobile-success)' : 'var(--mobile-accent)' }"
              ></span>
              <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate">
                {{ peer.deviceName }}
              </span>
              <span class="text-xs flex-shrink-0" style="color: var(--mobile-row-sub)">
                {{ shortFingerprint(peer.nodeId) }}
              </span>
            </div>
            <p class="text-xs mt-1 truncate" style="color: var(--mobile-row-sub)">
              {{ $t('peers.devices.online') }} · {{ peer.addr }}
            </p>
            <div class="flex items-center gap-1.5 mt-1.5 flex-wrap">
              <!-- 能力标记：文件传输有无一目了然 -->
              <span
                class="px-2 py-0.5 rounded-md text-xs font-medium"
                :style="
                  peer.fileTransfer
                    ? 'background: color-mix(in srgb, var(--mobile-accent) 14%, transparent); color: var(--mobile-text-primary)'
                    : 'border: 1px dashed var(--mobile-row-sub); color: var(--mobile-row-sub)'
                "
              >
                {{
                  peer.fileTransfer ? $t('peers.devices.capFileTransfer') : $t('peers.devices.capNone')
                }}
              </span>
              <span
                v-if="isConnected(peer.nodeId)"
                class="px-2 py-0.5 rounded-md text-xs font-medium"
                :style="'background: color-mix(in srgb, var(--mobile-success) 16%, transparent); color: var(--mobile-text-primary)'"
              >
                {{ $t('peers.devices.connected') }}
              </span>
              <span
                v-else-if="isConnecting(peer.nodeId)"
                class="text-xs"
                style="color: var(--mobile-row-sub)"
              >
                {{ $t('peers.devices.connecting') }}
              </span>
              <span
                v-else-if="dialError(peer.nodeId)"
                class="text-xs"
                style="color: var(--mobile-error)"
              >
                {{
                  dialError(peer.nodeId) === 'denied'
                    ? $t('peers.devices.denied')
                    : $t('peers.devices.unreachable')
                }}
              </span>
            </div>
          </div>

          <!-- 操作按钮：44px 最小触达；无能力节点禁用发起 -->
          <div class="flex flex-col gap-2 flex-shrink-0">
            <button
              class="min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors duration-200 active:opacity-80 disabled:opacity-40"
              :disabled="!peer.fileTransfer || isConnecting(peer.nodeId)"
              :style="
                isConnected(peer.nodeId)
                  ? 'color: var(--mobile-error); background: color-mix(in srgb, var(--mobile-error) 12%, transparent)'
                  : 'color: var(--mobile-text-on-accent); background: var(--mobile-accent)'
              "
              @click="
                isConnected(peer.nodeId) ? disconnect(peer.nodeId) : connect(peer.nodeId)
              "
            >
              {{
                isConnected(peer.nodeId)
                  ? $t('peers.devices.disconnect')
                  : isConnecting(peer.nodeId)
                    ? $t('peers.devices.connecting')
                    : $t('peers.devices.connect')
              }}
            </button>
            <!-- 浏览对端共享目录（issue 11）：仅已连接（互信）态可见，只读入口 -->
            <button
              v-if="isConnected(peer.nodeId) && peer.fileTransfer"
              class="min-h-[44px] px-4 rounded-xl text-sm font-medium transition-colors duration-200 active:opacity-80"
              style="
                color: var(--mobile-accent);
                background: color-mix(in srgb, var(--mobile-accent) 12%, transparent);
              "
              @click="browseFiles(peer)"
            >
              {{ $t('peers.files.browse') }}
            </button>
          </div>
        </div>
      </div>

      <p class="text-xs px-1 pt-1" style="color: var(--mobile-row-sub)">
        {{ $t('peers.devices.trustHint') }}
      </p>
    </div>
  </SettingsSubPage>
</template>

<script setup lang="ts">
/**
 * 对等网络设备列表二级页 — 发现缓存驱动的「看得见」入口（issue 08）
 *
 * 列表随节点上下线自动刷新（宿主 peer-devices-changed 推送）；具备文件传输
 * 能力的节点可发起连接——对端确认后进入已连接态；无能力节点可见但按钮禁用。
 * 信任撤销在 可信对端 子页。
 */
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import SettingsSubPage from '@/components/SettingsSubPage.vue'
import { usePeerDevices, type DiscoveredPeer } from '@/composables/usePeerDevices'

const router = useRouter()
const { peers, connectingIds, connectedIds, dialErrors, start, refresh, connect, disconnect } =
  usePeerDevices()

/** 进入对端远端文件页（issue 11；携带设备名供页头展示） */
function browseFiles(peer: DiscoveredPeer): void {
  void router.push({
    name: 'mobile-peer-files',
    params: { nodeId: peer.nodeId },
    query: { name: peer.deviceName },
  })
}

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

onMounted(async () => {
  await start()
  // 兜底：事件监听就绪后再拉一次，弥合 start 内首帧与监听注册的间隙
  await refresh()
})
</script>
