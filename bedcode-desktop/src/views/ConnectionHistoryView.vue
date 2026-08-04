<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-page px-8 h-14 flex items-center gap-4">
      <button
        @click="router.push('/devices')"
        class="flex items-center gap-1.5 text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        {{ t('desktop.device.historyBack') }}
      </button>
      <h2 class="text-[var(--font-size-title)] font-semibold text-[var(--text-primary)]">
        {{ t('desktop.device.historyTitle') }}
      </h2>
      <span v-if="deviceName" class="text-[var(--text-tertiary)] text-sm">{{ deviceName }}</span>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="bg-card rounded-card p-6 shadow-card animate-fade-slide-up">
        <div class="flex items-center justify-between mb-5">
          <h3 class="text-[var(--font-size-card-title)] font-semibold text-[var(--text-primary)]">
            {{ t('desktop.device.historyTitle') }}
          </h3>
          <Button
            v-if="history.length > 0"
            variant="ghost"
            size="sm"
            @click="showClearDialog = true"
          >
            {{ t('desktop.device.historyClear') }}
          </Button>
        </div>

        <div v-if="isLoading" class="text-center py-12 text-[var(--text-tertiary)]">
          {{ t('common.status.loading') }}
        </div>

        <div v-else-if="history.length === 0" class="text-center py-12">
          <svg class="w-12 h-12 mx-auto text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <p class="text-[var(--text-secondary)]">{{ t('desktop.device.historyEmpty') }}</p>
        </div>

        <div v-else class="space-y-3">
          <div
            v-for="entry in history"
            :key="entry.id"
            class="flex items-center justify-between p-4 bg-[var(--bg-hover)]/50 rounded-input"
          >
            <div class="flex items-center gap-3 min-w-0">
              <span
                :class="[
                  'w-2.5 h-2.5 rounded-full shrink-0',
                  entry.result === 'success' ? 'bg-green-500' : 'bg-red-400'
                ]"
              ></span>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span
                    :class="[
                      'text-xs px-2.5 py-0.5 rounded-tag font-medium',
                      methodBadgeClass(entry.authMethod)
                    ]"
                  >
                    {{ methodLabel(entry.authMethod) }}
                  </span>
                  <span
                    :class="[
                      'text-xs px-2.5 py-0.5 rounded-tag font-medium',
                      entry.result === 'success'
                        ? 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
                        : 'bg-[var(--color-danger-light)] text-red-500'
                    ]"
                  >
                    {{ resultLabel(entry.result) }}
                  </span>
                </div>
                <div class="flex items-center gap-3 text-[var(--text-tertiary)] text-xs mt-1.5">
                  <span>{{ t('desktop.device.historyConnectedAt') }} {{ formatTime(entry.connectedAt) }}</span>
                  <span v-if="entry.disconnectedAt">|</span>
                  <span v-if="entry.disconnectedAt">{{ t('desktop.device.historyDisconnectedAt') }} {{ formatTime(entry.disconnectedAt) }}</span>
                  <span v-if="entry.address">|</span>
                  <span v-if="entry.address" class="font-mono">{{ entry.address }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Clear Confirm Dialog -->
    <Modal v-model="showClearDialog" :title="t('desktop.device.historyClear')" size="sm">
      <p class="text-[var(--text-primary)]">{{ t('desktop.device.historyClearConfirm') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showClearDialog = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmClearHistory">{{ t('common.button.clear') }}</Button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * ConnectionHistoryView - 桌面端设备连接历史
 *
 * 展示单个已配对设备的连接历史（认证方式、结果、连接/断开时间），
 * 支持一键清空。数据来自 list_connection_history / delete_connection_history 命令。
 */
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useDeviceStore } from '@/stores/device'
import { useDesktopCommands, type ConnectionHistoryEntry } from '@/composables/useDesktopCommands'
import { useToast } from '@/composables/useToast'
import Button from '@/components/Button.vue'
import Modal from '@/components/Modal.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const deviceStore = useDeviceStore()
const commands = useDesktopCommands()
const toast = useToast()

const deviceId = computed(() => route.params.id as string)
const deviceName = computed(() => {
  const device = deviceStore.pairedDevices.find(d => d.id === deviceId.value)
  return device?.deviceName ?? ''
})

const history = ref<ConnectionHistoryEntry[]>([])
const isLoading = ref(false)
const showClearDialog = ref(false)

function methodLabel(method: string): string {
  const keyMap: Record<string, string> = {
    pairing_code: 'historyMethodPairingCode',
    qr: 'historyMethodQr',
    biometric: 'historyMethodBiometric',
    jwt: 'historyMethodJwt',
  }
  return t(`desktop.device.${keyMap[method] ?? 'historyMethodUnknown'}`)
}

function resultLabel(result: string): string {
  return result === 'success'
    ? t('desktop.device.historyResultSuccess')
    : t('desktop.device.historyResultFailed')
}

function methodBadgeClass(method: string): string {
  switch (method) {
    case 'biometric':
      return 'bg-[var(--color-success-light)] text-green-600 dark:text-green-400'
    case 'qr':
      return 'bg-[var(--bg-hover)] text-blue-600 dark:text-blue-400'
    case 'pairing_code':
      return 'bg-[var(--color-warning-light)] text-amber-600 dark:text-amber-400'
    default:
      return 'bg-[var(--bg-hover)] text-[var(--text-secondary)]'
  }
}

function formatTime(timeStr: string): string {
  const date = new Date(timeStr)
  if (isNaN(date.getTime())) return t('common.status.unknown')
  return date.toLocaleString('zh-CN', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

async function loadHistory() {
  isLoading.value = true
  try {
    history.value = await commands.listConnectionHistory(deviceId.value)
  } catch (e) {
    console.error('加载连接历史失败:', e)
    toast.error(t('desktop.device.historyLoadFailed'))
  } finally {
    isLoading.value = false
  }
}

async function confirmClearHistory() {
  try {
    await commands.deleteConnectionHistory(deviceId.value)
    history.value = []
    showClearDialog.value = false
    toast.success(t('desktop.device.historyCleared'))
  } catch (e) {
    console.error('清空连接历史失败:', e)
    toast.error(t('desktop.device.historyLoadFailed'))
  }
}

onMounted(async () => {
  await deviceStore.loadPairedDevices()
  await loadHistory()
})
</script>
