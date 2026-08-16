<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左返回+设备名+统计，右清空 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3 min-w-0">
        <button
          class="flex items-center gap-1 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          @click="router.push('/devices')"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 19l-7-7 7-7" />
          </svg>
          {{ t('desktop.device.historyBack') }}
        </button>
        <span class="text-[var(--text-tertiary)]">/</span>
        <h2 class="text-sm font-semibold text-[var(--text-primary)] truncate">
          {{ deviceName || t('desktop.device.historyTitle') }}
        </h2>
        <!-- 统计计数 -->
        <span v-if="!isLoading && history.length > 0" class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] whitespace-nowrap">
          {{ history.length }} total · <span class="text-green-700 dark:text-green-400">{{ successCount }} ok</span> · <span class="text-red-700 dark:text-red-400">{{ failCount }} fail</span>
        </span>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="history" />
        <button
          v-if="history.length > 0"
          class="wb-btn-ghost"
        @click="showClearDialog = true"
      >
        {{ t('desktop.device.historyClear') }}
      </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-6">
      <!-- 加载态 -->
      <div v-if="isLoading" class="flex flex-col items-center justify-center py-20">
        <svg class="w-5 h-5 animate-spin text-[var(--text-secondary)] mb-3" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"></path>
        </svg>
        <p class="wb-mono text-xs text-[var(--text-secondary)]">{{ t('common.status.loading') }}</p>
      </div>

      <!-- 空态 -->
      <div v-else-if="history.length === 0" class="flex flex-col items-center justify-center py-20">
        <svg class="w-7 h-7 text-[var(--text-tertiary)] mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <p class="wb-mono text-xs text-[var(--text-secondary)]">{{ t('desktop.device.historyEmpty') }}</p>
      </div>

      <!-- 按日期分组的 section -->
      <div v-else class="space-y-6 max-w-3xl">
        <section v-for="group in groups" :key="group.date" class="history-group">
          <!-- 小号全大写 letterspacing 分组标题 -->
          <h3 class="wb-section-title font-mono tracking-[0.12em]">
            {{ group.date }} · {{ group.entries.length }}
          </h3>
          <div class="border border-[var(--border)] rounded-[10px] bg-[var(--bg-card)] divide-y divide-[var(--border)] overflow-hidden">
            <div
              v-for="entry in group.entries"
              :key="entry.id"
              class="history-entry px-4 py-3 flex items-center gap-3 hover:bg-[var(--bg-hover)] transition-colors"
            >
              <span
                :class="[
                  'w-2 h-2 rounded-full shrink-0',
                  entry.result === 'success' ? 'bg-green-500' : 'bg-red-400'
                ]"
              ></span>
              <span class="text-xs font-medium text-[var(--text-primary)] w-16 shrink-0">{{ methodLabel(entry.authMethod) }}</span>
              <span class="wb-mono text-[var(--text-secondary)] truncate flex-1 min-w-0">
                {{ entry.address ?? '—' }}
              </span>
              <span class="wb-mono text-[var(--text-tertiary)] tabular-nums whitespace-nowrap">
                {{ clockTime(entry.connectedAt) }}<template v-if="entry.disconnectedAt"> → {{ clockTime(entry.disconnectedAt) }}</template>
              </span>
              <span
                :class="[
                  'wb-mono text-[calc(11px*var(--ui-scale))] w-12 text-right shrink-0',
                  entry.result === 'success' ? 'text-green-700 dark:text-green-400' : 'text-red-700 dark:text-red-400'
                ]"
              >
                {{ resultLabel(entry.result) }}
              </span>
            </div>
          </div>
        </section>
      </div>
    </div>

    <!-- 清空确认对话框 -->
    <Modal v-model="showClearDialog" :title="t('desktop.device.historyClear')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">{{ t('desktop.device.historyClearConfirm') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showClearDialog = false">{{ t('common.button.cancel') }}</button>
          <button class="wb-btn-primary" :class="{ 'bg-[var(--color-danger)]': true }" @click="confirmClearHistory">{{ t('common.button.clear') }}</button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * ConnectionHistoryView — 桌面端设备连接历史
 * Warm Workbench 风格：工具栏统计 + 按日期分组列表；清空走真实删除 + 确认对话框
 */
import { ref, computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { useDeviceStore } from '@/stores/device'
import { useDesktopCommands, type ConnectionHistoryEntry } from '@/composables/useDesktopCommands'
import { useToast } from '@/composables/useToast'
import Modal from '@/components/Modal.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'

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

const successCount = computed(() => history.value.filter(e => e.result === 'success').length)
const failCount = computed(() => history.value.length - successCount.value)

/** 按连接日期分组，保持原列表顺序（后端已按时间倒序） */
const groups = computed(() => {
  const map = new Map<string, ConnectionHistoryEntry[]>()
  for (const entry of history.value) {
    const key = dayKey(entry.connectedAt)
    if (!map.has(key)) map.set(key, [])
    map.get(key)!.push(entry)
  }
  return Array.from(map.entries()).map(([date, entries]) => ({ date, entries }))
})

function dayKey(timeStr: string): string {
  const date = new Date(timeStr)
  if (isNaN(date.getTime())) return t('common.status.unknown')
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

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

function clockTime(timeStr: string): string {
  const date = new Date(timeStr)
  if (isNaN(date.getTime())) return t('common.status.unknown')
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
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

<style scoped>
/*
 * 长列表优化：content-visibility: auto 跳过屏幕外元素的渲染
 *
 * 收益预估（100+ 条连接历史场景）：
 * - 初次渲染：跳屏外 section 的 layout/paint，FPS 提升 3-5x
 * - 大列表滚动：滚动时仅渲染视口内 section，主线程压力下降
 * - 代价：首次快速滚动到屏幕外时会有轻微 "弹出" 动画
 *
 * contain-intrinsic-size 预估行高，避免滚动条跳动
 * - 标题 ~32px
 * - 每条 entry ~52px (含内边距)
 * - 容器预估总高 = 32 + 52 * 假设平均 5 条 = 292px
 */
.history-group {
  content-visibility: auto;
  contain-intrinsic-size: 0 292px;
}

.history-entry {
  /* 单条 entry 单独启用 skip，配合整组 skip，双重保险 */
  content-visibility: auto;
  contain-intrinsic-size: 0 52px;
}
</style>
