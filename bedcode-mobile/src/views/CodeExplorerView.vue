<template>
  <div class="code-explorer" :style="explorerStyle">
    <FileExplorer
      :session-id="sessionId"
      mode="emit"
      :default-show-sidebar="true"
      :title="configName"
      @close="handleBack"
      @navigate-settings="handleNavigateSettings"
    >
      <template #header-left>
        <button class="back-btn" @click="handleBack">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
      </template>
      <template #header-right>
        <button class="settings-btn" @click="showSettings = true" :title="t('mobile.codeViewer.settingsTitle')">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </template>
    </FileExplorer>

    <!-- Settings Modal -->
    <CodeViewerSettingsModal
      :visible="showSettings"
      @close="showSettings = false"
      @confirm="onSettingsConfirm"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * CodeExplorerView - 代码查看页面
 *
 * 基于 FileExplorer 组件的全屏封装
 */

import { ref, computed, inject, type Ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useMobileConnection } from '@/composables/useMobileConnection'
import FileExplorer from '@/components/FileExplorer.vue'
import CodeViewerSettingsModal from '@/components/CodeViewerSettingsModal.vue'

const router = useRouter()
const route = useRoute()
const connection = useMobileConnection()
const { t } = useI18n()
const safeArea = inject<Ref<{ top: number; bottom: number }>>('safeArea')!

const showSettings = ref(false)

const sessionId = computed(() => route.params.id as string)

const configName = computed(() => {
  const session = connection.activeSessions.value.find(
    (s: any) => s.id === sessionId.value
  )
  if (!session) return t('mobile.codeViewer.title')
  const configId = session.config_id || session.configId
  const config = connection.sessionConfigs.value.find(c => c.id === configId)
  return config?.name || t('mobile.codeViewer.title')
})

const explorerStyle = computed(() => ({
  paddingTop: `${safeArea.value.top}px`,
}))

function handleBack() {
  router.back()
}

/** 未连接（base URL 缺失）→ 引导去连接设置页 */
function handleNavigateSettings() {
  router.push({ name: 'mobile-settings-connection' })
}

function onSettingsConfirm() {
  // 主题变化由 FileExplorer 内部 watch 处理
  // 字体大小、tab 缩进、行号通过 CSS 变量实时生效
}
</script>

<style scoped>
.code-explorer {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--mobile-bg-primary);
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 1;
  overflow: hidden;
}

.back-btn {
  padding: 0.375rem;
  margin-left: -0.375rem;
  color: var(--mobile-text-secondary);
  background: none;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.back-btn:active {
  color: var(--mobile-accent);
}

.settings-btn {
  padding: 0.375rem;
  border-radius: 0.375rem;
  background: none;
  border: none;
  color: var(--mobile-text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease;
}

.settings-btn:active {
  color: var(--mobile-accent);
}
</style>
