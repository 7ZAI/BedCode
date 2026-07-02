<template>
  <div class="p-4 space-y-3 border-b border-slate-200 dark:border-dark-700 bg-slate-50 dark:bg-dark-800">
    <h4 class="text-sm font-semibold text-slate-700 dark:text-dark-300">模型配置</h4>

    <div v-for="provider in providers" :key="provider.name">
      <div
        :class="[
          'flex items-center justify-between p-3 rounded-lg border cursor-pointer transition-colors',
          provider.name === activeProviderName
            ? 'border-primary-500 bg-primary-50 dark:bg-primary-900/20'
            : 'border-slate-200 dark:border-dark-600 bg-white dark:bg-dark-800 hover:border-slate-300 dark:hover:border-dark-500'
        ]"
        @click="emit('setActive', provider.name)"
      >
        <div>
          <div class="text-sm font-medium text-slate-800 dark:text-white">{{ provider.name }}</div>
          <div class="text-xs text-slate-400 dark:text-dark-500">{{ provider.model }}</div>
        </div>
        <div class="flex items-center gap-2">
          <span v-if="provider.name === activeProviderName" class="text-xs text-primary-600 dark:text-primary-400">当前</span>
          <button
            class="text-xs text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300"
            @click.stop="emit('remove', provider.name)"
          >
            删除
          </button>
        </div>
      </div>
    </div>

    <div class="border border-dashed border-slate-300 dark:border-dark-600 rounded-lg p-3 space-y-2">
      <h5 class="text-xs font-medium text-slate-500 dark:text-dark-400">从预设添加</h5>
      <div class="flex flex-wrap gap-2">
        <button
          v-for="preset in presets"
          :key="preset.name"
          class="px-2 py-1 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
          @click="selectPreset(preset)"
        >
          {{ preset.name }}
        </button>
      </div>

      <div v-if="editingPreset" class="space-y-2 pt-2">
        <input v-model="form.name" type="text" placeholder="名称" class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none" />
        <input v-model="form.apiKey" type="password" placeholder="API Key" class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none" />
        <input v-model="form.baseUrl" type="text" placeholder="Base URL" class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none" />
        <input v-model="form.model" type="text" placeholder="模型名称" class="w-full bg-white dark:bg-dark-700 border border-slate-200 dark:border-dark-600 rounded px-2 py-1.5 text-xs text-slate-900 dark:text-white outline-none" />
        <div class="flex gap-2">
          <button
            :disabled="!form.name || !form.apiKey"
            class="px-3 py-1.5 text-xs bg-primary-600 hover:bg-primary-700 disabled:opacity-50 text-white rounded transition-colors"
            @click="handleAdd"
          >
            添加
          </button>
          <button
            class="px-3 py-1.5 text-xs bg-slate-100 dark:bg-dark-700 text-slate-600 dark:text-dark-300 rounded hover:bg-slate-200 dark:hover:bg-dark-600 transition-colors"
            @click="editingPreset = null"
          >
            取消
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive } from 'vue'
import type { ApiProvider, ProviderPreset } from '../types'
import { PROVIDER_PRESETS } from '../types'

const props = defineProps<{
  providers: ApiProvider[]
  activeProviderName: string
}>()

const emit = defineEmits<{
  setActive: [name: string]
  remove: [name: string]
  add: [provider: ApiProvider]
}>()

const presets = PROVIDER_PRESETS
const editingPreset = ref<ProviderPreset | null>(null)
const form = reactive({ name: '', apiKey: '', baseUrl: '', model: '' })

function selectPreset(preset: ProviderPreset): void {
  editingPreset.value = preset
  form.name = preset.name
  form.apiKey = ''
  form.baseUrl = preset.baseUrl
  form.model = preset.model
}

function handleAdd(): void {
  if (!form.name || !form.apiKey) return
  emit('add', { name: form.name, apiKey: form.apiKey, baseUrl: form.baseUrl, model: form.model })
  editingPreset.value = null
  form.name = ''
  form.apiKey = ''
  form.baseUrl = ''
  form.model = ''
}
</script>
