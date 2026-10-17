<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="visible"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="cancel"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Dialog -->
        <div
          class="relative w-full max-w-md rounded-card shadow-2xl border bg-card border-[var(--border)]"
        >
          <!-- Header -->
          <div class="px-5 py-4 border-b border-[var(--border)]">
            <h3 class="text-base font-semibold text-[var(--text-primary)]">
              {{ t('settings.system.factoryResetTitle') }}
            </h3>
          </div>

          <!-- Body -->
          <div class="px-5 py-4 space-y-3">
            <p class="text-sm text-[var(--text-secondary)]">
              {{ t('settings.system.factoryResetDesc') }}
            </p>
            <ul class="text-xs text-[var(--text-secondary)] space-y-1 list-disc list-inside">
              <li>{{ t('settings.system.factoryResetClears') }}</li>
              <li>{{ t('settings.system.factoryResetKeeps') }}</li>
              <li>{{ t('settings.system.factoryResetRestartHint') }}</li>
            </ul>
          </div>

          <!-- Footer -->
          <div class="px-5 py-3 border-t border-[var(--border)] flex justify-end gap-2">
            <Button variant="ghost" size="sm" :disabled="busy" @click="cancel">
              {{ t('common.button.cancel') }}
            </Button>
            <Button variant="danger" size="sm" :loading="busy" @click="confirmReset">
              {{ t('settings.system.factoryResetConfirm') }}
            </Button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * 恢复初始设置确认弹窗 — 危险操作二次确认
 *
 * 确认后调宿主命令 `reset_app_settings`：清空主库授权记录 / 授权策略 / 引擎
 * 配置与插件激活态、删除用户 config.properties（下次启动还原默认）；**保留**
 * wasm 应用插件私有库、凭据与对端信任。命令返回后提示重启应用完全生效。
 */
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useI18n } from 'vue-i18n'
import Button from '@/components/Button.vue'
import { useToast } from '@/composables/useToast'
import { logger } from '@/utils/frontendLogger'

const { t } = useI18n()
const toast = useToast()
const busy = ref(false)

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ 'update:visible': [value: boolean] }>()

function cancel() {
  if (busy.value) return
  emit('update:visible', false)
}

async function confirmReset() {
  busy.value = true
  try {
    await invoke('reset_app_settings')
    toast.success(t('settings.system.factoryResetDone'))
    emit('update:visible', false)
  } catch (e) {
    logger.error('[FactoryReset] reset_app_settings failed:', e)
    toast.error(t('settings.system.factoryResetFailed'))
  } finally {
    busy.value = false
  }
}
</script>
