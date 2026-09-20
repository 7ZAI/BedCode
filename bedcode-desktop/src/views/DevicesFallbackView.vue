<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg class="w-4 h-4 text-[var(--text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z"
          />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('desktop.device.title') }}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <button class="wb-btn-ghost" @click="refresh">
          {{ t('common.button.refresh') }}
        </button>
      </div>
    </div>

    <!-- ==================== 内容区：兜底提示 + 基本设备管理 ==================== -->
    <div class="flex-1 overflow-auto px-6 py-6 space-y-6 max-w-3xl">
      <p
        class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] leading-relaxed rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-4 py-3"
      >
        {{ t('desktop.device.fallbackNotice') }}
      </p>

      <section>
        <h3 class="wb-section-title">
          {{ t('desktop.device.pairedTitle') }}
          <span class="text-[var(--text-tertiary)]">·</span> {{ devices.length }}
        </h3>
        <p
          v-if="devices.length === 0"
          class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] px-1 py-2"
        >
          {{ t('desktop.device.noPaired') }}
        </p>
        <div v-else class="space-y-2">
          <article
            v-for="device in devices"
            :key="device.id"
            class="px-4 py-3 rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] flex items-center gap-3 min-w-0"
          >
            <p
              class="flex-1 min-w-0 text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
            >
              {{ device.deviceName }}
            </p>
            <span class="wb-mono text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{
              device.address
            }}</span>
            <button
              class="h-7 px-2.5 rounded-[6px] border border-transparent wb-mono text-[calc(11px*var(--ui-scale))] uppercase tracking-wide text-[var(--text-tertiary)] hover:border-[var(--border)] hover:text-red-500 transition-colors"
              @click="removeDevice(device.id)"
            >
              {{ t('common.button.remove') }}
            </button>
          </article>
        </div>
      </section>
    </div>

    <!-- 移除设备确认 -->
    <Modal v-model="showRemoveDialog" :title="t('desktop.device.confirmRemove')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('desktop.device.confirmRemoveMsg') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showRemoveDialog = false">
            {{ t('common.button.cancel') }}
          </button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmRemove">
            {{ t('common.button.remove') }}
          </button>
        </div>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * DevicesFallbackView — 设备与配对兜底壳（票 14）
 *
 * 设备配对 / 已配对设备 / 连接历史三块视图搬入 `com.bedcode.session` 插件后，宿主
 * 的 `/devices` 路由退为兜底：插件处于 Activated / Degraded 时由路由守卫重定向到
 * 插件贡献目录（票 02 的让位判据）；插件未激活 / error / 停用时渲染本页。
 *
 * 口径（沿用票 13 会话页兜底壳的用户裁决）：**最小壳** = 一句说明 + 基本设备管理
 * （列出已配对设备、可撤销），不含配对码 / QR / 历史等完整功能面——那些能力只在
 * 插件侧，宿主不再维护第二份实现，避免双份漂移。
 */
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { logger } from '@/utils/frontendLogger'
import { useDeviceStore } from '@/stores/device'
import Modal from '@/components/Modal.vue'

const { t } = useI18n()
const deviceStore = useDeviceStore()

// Pinia setup store 的 ref 在 store 上是解包属性：直接取值会拿到快照（非响应式引用），
// 必须在 computed 内取值才能跟随 store 更新
const devices = computed(() => deviceStore.pairedDevices)
const showRemoveDialog = ref(false)
const pendingDeviceId = ref<string | null>(null)

async function refresh() {
  try {
    await deviceStore.loadPairedDevices()
  } catch (e) {
    logger.error('[DevicesFallbackView] load paired devices failed:', e)
  }
}

function removeDevice(deviceId: string) {
  pendingDeviceId.value = deviceId
  showRemoveDialog.value = true
}

async function confirmRemove() {
  if (!pendingDeviceId.value) return
  try {
    await deviceStore.removeDevice(pendingDeviceId.value)
  } catch (e) {
    logger.error('[DevicesFallbackView] remove device failed:', e)
  } finally {
    showRemoveDialog.value = false
    pendingDeviceId.value = null
  }
}

onMounted(refresh)
</script>
