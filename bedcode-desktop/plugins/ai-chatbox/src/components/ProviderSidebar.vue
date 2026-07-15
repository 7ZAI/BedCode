<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)] border-r border-[var(--border)]">
    <!-- 预设供应商 -->
    <div class="p-3">
      <h4 class="px-2 mb-2 text-xs font-medium text-[var(--text-tertiary)] uppercase tracking-wider">
        {{ t('desktop.plugin.aiChatbox.presetProviders') }}
      </h4>
      <ul class="space-y-0.5">
        <li v-for="preset in presets" :key="preset.name">
          <button
            :class="[
              'w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-sm transition-colors text-left',
              selectedPresetName === preset.name
                ? 'bg-brand-light text-[var(--text-brand)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
            ]"
            @click="emit('selectPreset', preset)"
          >
            <span class="truncate">{{ preset.name }}</span>
          </button>
        </li>
      </ul>
    </div>

    <!-- 分隔线 -->
    <div class="mx-3 border-t border-[var(--border)]"></div>

    <!-- 自定义供应商 -->
    <div class="flex-1 p-3 overflow-y-auto">
      <h4 class="px-2 mb-2 text-xs font-medium text-[var(--text-tertiary)] uppercase tracking-wider">
        {{ t('desktop.plugin.aiChatbox.customProviders') }}
      </h4>
      <ul v-if="providers.length > 0" class="space-y-0.5">
        <li v-for="provider in providers" :key="provider.id">
          <button
            :class="[
              'w-full flex items-center justify-between px-2 py-1.5 rounded-md text-sm transition-colors text-left',
              selectedProviderId === provider.id
                ? 'bg-brand-light text-[var(--text-brand)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
            ]"
            @click="emit('selectProvider', provider.id)"
          >
            <span class="truncate">{{ provider.name }}</span>
            <span class="text-xs text-[var(--text-tertiary)] shrink-0 ml-1">
              {{ provider.models.length }}
            </span>
          </button>
        </li>
      </ul>
      <p v-else class="px-2 text-xs text-[var(--text-tertiary)]">
        {{ t('desktop.plugin.aiChatbox.noProvider') }}
      </p>
    </div>

    <!-- 添加按钮 -->
    <div class="p-3 border-t border-[var(--border)]">
      <button
        :class="[
          'w-full flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
          isAddMode
            ? 'bg-brand-light text-[var(--text-brand)]'
            : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
        ]"
        @click="emit('addNew')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
        {{ t('desktop.plugin.aiChatbox.addCustomProvider') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商侧边栏 — 预设列表 + 已添加供应商列表
 */
import { useI18n } from 'vue-i18n'
import type { ApiProvider, ProviderPreset } from '../types'
import { PROVIDER_PRESETS } from '../types'

const { t } = useI18n()

defineProps<{
  providers: ApiProvider[]
  selectedProviderId: string
  selectedPresetName: string
  isAddMode: boolean
}>()

const emit = defineEmits<{
  selectProvider: [id: string]
  selectPreset: [preset: ProviderPreset]
  addNew: []
}>()

const presets = PROVIDER_PRESETS
</script>
