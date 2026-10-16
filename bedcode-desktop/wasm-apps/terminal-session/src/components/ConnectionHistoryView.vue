<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左返回+设备名+统计，右清空 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-3 min-w-0">
        <button
          class="flex items-center gap-1 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          @click="backToDevices"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M15 19l-7-7 7-7"
            />
          </svg>
          {{ t('pairing.history.back') }}
        </button>
        <span class="text-[var(--text-tertiary)]">/</span>
        <h2 class="text-sm font-semibold text-[var(--text-primary)] truncate">
          {{ deviceName || t('pairing.history.title') }}
        </h2>
        <!-- 统计计数 -->
        <span
          v-if="!isLoading && entries.length > 0"
          class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] whitespace-nowrap"
        >
          {{ t('pairing.history.statistics', { total: entries.length, ok: successCount, fail: failCount }) }}
        </span>
      </div>
      <div class="flex items-center gap-2">
        <Select
          v-if="devices.length > 0"
          :model-value="deviceId"
          :options="deviceOptions"
          :placeholder="t('pairing.history.title')"
          size="sm"
          class="w-[180px]"
          @update:model-value="selectDevice"
        />
        <button v-if="entries.length > 0" class="wb-btn-ghost" @click="showClearDialog = true">
          {{ t('pairing.history.clear') }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto px-6 py-6">
      <!-- 加载态 -->
      <div v-if="isLoading" class="flex flex-col items-center justify-center py-20">
        <svg
          class="w-5 h-5 animate-spin text-[var(--text-secondary)] mb-3"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            class="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            stroke-width="2"
          ></circle>
          <path
            class="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
          ></path>
        </svg>
        <p class="wb-mono text-xs text-[var(--text-secondary)]">
          {{ t('pairing.status.loading') }}
        </p>
      </div>

      <!-- 空态 -->
      <div v-else-if="entries.length === 0" class="flex flex-col items-center justify-center py-20">
        <svg
          class="w-7 h-7 text-[var(--text-tertiary)] mb-3"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <p class="wb-mono text-xs text-[var(--text-secondary)]">
          {{ t('pairing.history.empty') }}
        </p>
      </div>

      <!-- 按日期分组的 section -->
      <div v-else class="space-y-6 max-w-3xl">
        <section v-for="group in groups" :key="group.date" class="history-group">
          <h3 class="wb-section-title font-mono tracking-[0.12em]">
            {{ group.date }} · {{ group.entries.length }}
          </h3>
          <div
            class="border border-[var(--border)] rounded-[10px] bg-[var(--bg-card)] divide-y divide-[var(--border)] overflow-hidden"
          >
            <div
              v-for="entry in group.entries"
              :key="entry.id"
              class="history-entry px-4 py-3 flex items-center gap-3 hover:bg-[var(--bg-hover)] transition-colors"
            >
              <span
                :class="[
                  'w-2 h-2 rounded-full shrink-0',
                  entry.result === 'success' ? 'bg-green-500' : 'bg-red-400',
                ]"
              ></span>
              <span class="text-xs font-medium text-[var(--text-primary)] w-16 shrink-0">{{
                methodLabel(entry.authMethod)
              }}</span>
              <span class="wb-mono text-[var(--text-secondary)] truncate flex-1 min-w-0">
                {{ entry.address ?? '—' }}
              </span>
              <span class="wb-mono text-[var(--text-tertiary)] tabular-nums whitespace-nowrap">
                {{ clockLabel(entry.connectedAt)
                }}<template v-if="entry.disconnectedAt">
                  → {{ clockLabel(entry.disconnectedAt) }}</template
                >
              </span>
              <span
                :class="[
                  'wb-mono text-[calc(11px*var(--ui-scale))] w-12 text-right shrink-0',
                  entry.result === 'success'
                    ? 'text-green-700 dark:text-green-400'
                    : 'text-red-700 dark:text-red-400',
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
    <PluginModal v-model="showClearDialog" :title="t('pairing.history.clear')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('pairing.history.clearConfirm') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showClearDialog = false">
            {{ t('pairing.button.cancel') }}
          </button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmClearHistory">
            {{ t('pairing.button.clear') }}
          </button>
        </div>
      </template>
    </PluginModal>
  </div>
</template>

<script setup lang="ts">
/**
 * ConnectionHistoryView — 连接历史页面（宿主 `ConnectionHistoryView.vue` 的插件版，票 14）
 *
 * 入口形态：本插件侧边栏的第二个目录（`session.history`，order 101，紧随设备与配对）；
 * 设备列表页的「历史」按钮经宿主 router 跳转到本页并带 `?deviceId=`。
 * 无 `deviceId` 时（例如从侧边栏直接点入）用设备下拉选择，默认选第一台已配对设备。
 *
 * 取数红线（spec D2）：只经插件命令通道（`session.devices.history-*` /
 * `session.devices.paired-list`），不直调宿主 `list_connection_history`。
 */
import { computed, inject, onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'
import { getRouter, type PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import PluginModal from './PluginModal.vue'
import { useConnectionHistory } from '../composables/useConnectionHistory'
import type { PairedDeviceInfo } from '../composables/useDeviceCenter'

const context = inject<PluginContext>('pluginContext')!
const { t } = context.i18n

const {
  entries,
  isLoading,
  successCount,
  failCount,
  groups,
  clockTime,
  methodKeySuffix,
  load,
  clear,
} = useConnectionHistory(context)

const showClearDialog = ref(false)
const devices = ref<PairedDeviceInfo[]>([])

/** 当前设备 id：来自路由 query（设备列表跳转）或本地下拉选择 */
const routeDeviceId = computed(() => {
  const query = getRouter().currentRoute.value.query.deviceId
  return typeof query === 'string' ? query : ''
})
const pickedDeviceId = ref('')
const deviceId = computed(() => pickedDeviceId.value || routeDeviceId.value)

const deviceOptions = computed(() =>
  devices.value.map((d) => ({ value: d.id, label: d.deviceName })),
)
const deviceName = computed(
  () => devices.value.find((d) => d.id === deviceId.value)?.deviceName ?? '',
)

/** 认证方式展示文案（未知方式兜底 unknown，与宿主同映射） */
function methodLabel(authMethod: string): string {
  return t(`pairing.history.method.${methodKeySuffix(authMethod)}`)
}

/** 结果展示文案 */
function resultLabel(result: string): string {
  return result === 'success'
    ? t('pairing.history.result.success')
    : t('pairing.history.result.failed')
}

/** 时刻展示：非法时间回退 unknown 文案 */
function clockLabel(timeStr: string): string {
  const value = clockTime(timeStr)
  return value === 'unknown' ? t('pairing.status.unknown') : value
}

/** 设备下拉切换：写入本地选择并重新取数 */
function selectDevice(value: string | number) {
  pickedDeviceId.value = String(value)
  void loadHistory()
}

/** 返回设备与配对页（同一插件的另一个侧边栏目录） */
function backToDevices() {
  void getRouter().push({
    name: 'plugin-sidebar-view',
    params: { pluginId: context.id, viewId: 'session.pairing' },
  })
}

async function loadDevices() {
  const list = (await context.commands.execute('session.devices.paired-list', {})) as
    | PairedDeviceInfo[]
    | undefined
  devices.value = Array.isArray(list) ? list : []
  // 无路由参数时默认选第一台设备（与「从侧边栏点进来也能看到内容」一致）
  if (!deviceId.value && devices.value.length > 0) {
    pickedDeviceId.value = devices.value[0].id
  }
}

async function loadHistory() {
  if (!deviceId.value) return
  try {
    await load(deviceId.value)
  } catch (e) {
    console.error('[Device Center] load connection history failed:', e)
    toast.error(t('pairing.history.loadFailed'))
  }
}

async function confirmClearHistory() {
  try {
    await clear(deviceId.value)
    showClearDialog.value = false
    toast.success(t('pairing.history.cleared'))
  } catch (e) {
    console.error('[Device Center] clear connection history failed:', e)
    toast.error(t('pairing.history.loadFailed'))
  }
}

onMounted(async () => {
  try {
    await loadDevices()
  } catch (e) {
    console.error('[Device Center] load paired devices failed:', e)
    toast.error(t('pairing.error.loadFailed'))
  }
  await loadHistory()
})
</script>

<style scoped>
/*
 * 长列表优化：content-visibility: auto 跳过屏幕外元素的渲染（与宿主原页同一套优化）
 * contain-intrinsic-size 预估行高（标题 ~32px + 每条 entry ~52px），避免滚动条跳动
 */
.history-group {
  content-visibility: auto;
  contain-intrinsic-size: 0 292px;
}

.history-entry {
  content-visibility: auto;
  contain-intrinsic-size: 0 52px;
}
</style>
