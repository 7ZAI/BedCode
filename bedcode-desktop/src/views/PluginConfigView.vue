<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-slate-200 dark:border-dark-700 px-6 py-3 h-12 flex items-center shadow-sm dark:shadow-none">
      <!-- Breadcrumb -->
      <div class="flex items-center gap-2 text-sm">
        <router-link to="/plugins" class="text-primary-600 dark:text-primary-400 hover:underline">
          {{ $t('desktop.plugin.title') }}
        </router-link>
        <span class="text-slate-400 dark:text-dark-500">›</span>
        <span class="text-slate-700 dark:text-dark-300">
          {{ pluginInfo ? $t('desktop.plugin.configTitle', { name: pluginInfo.name }) : '...' }}
        </span>
      </div>
    </header>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-lg mx-auto">
        <!-- Loading -->
        <div v-if="loading" class="flex items-center justify-center py-12">
          <Spinner />
        </div>

        <!-- Not Activated -->
        <div v-else-if="!pluginInfo || !isActivatedState(pluginInfo.state)" class="py-12 text-center text-slate-500 dark:text-dark-400">
          {{ $t('desktop.plugin.pluginNotActivated') }}
        </div>

        <!-- No Configuration -->
        <div v-else-if="!configSchema" class="py-12 text-center text-slate-500 dark:text-dark-400">
          {{ pluginInfo.name }} — {{ $t('desktop.plugin.noConfigAvailable') }}
        </div>

        <!-- Config Form -->
        <div v-else class="bg-white dark:bg-dark-800 rounded-lg border border-slate-200 dark:border-dark-700 p-6 shadow-sm dark:shadow-none">
          <h3 class="text-base font-semibold mb-4">{{ pluginInfo.name }}</h3>

          <div class="space-y-4">
            <div v-for="(prop, key) in configSchema.properties" :key="key">
              <!-- String with enum → Select -->
              <template v-if="prop.type === 'string' && prop.enum">
                <label class="block text-sm font-medium text-slate-700 dark:text-dark-300 mb-1">{{ prop.title }}</label>
                <select
                  v-model="configValues[key]"
                  class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white focus:border-primary-500 outline-none"
                >
                  <option v-for="opt in prop.enum" :key="opt" :value="opt">{{ opt }}</option>
                </select>
                <p v-if="prop.description" class="text-xs text-slate-400 dark:text-dark-500 mt-1">{{ prop.description }}</p>
              </template>

              <!-- String → Text Input -->
              <template v-else-if="prop.type === 'string'">
                <label class="block text-sm font-medium text-slate-700 dark:text-dark-300 mb-1">{{ prop.title }}</label>
                <input
                  v-model="configValues[key]"
                  type="text"
                  class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white focus:border-primary-500 outline-none"
                />
                <p v-if="prop.description" class="text-xs text-slate-400 dark:text-dark-500 mt-1">{{ prop.description }}</p>
              </template>

              <!-- Number → Number Input -->
              <template v-else-if="prop.type === 'number'">
                <label class="block text-sm font-medium text-slate-700 dark:text-dark-300 mb-1">{{ prop.title }}</label>
                <input
                  v-model.number="configValues[key]"
                  type="number"
                  class="w-full bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 rounded-lg px-3 py-2 text-sm text-slate-900 dark:text-white focus:border-primary-500 outline-none"
                />
                <p v-if="prop.description" class="text-xs text-slate-400 dark:text-dark-500 mt-1">{{ prop.description }}</p>
              </template>

              <!-- Boolean → Toggle -->
              <template v-else-if="prop.type === 'boolean'">
                <div class="flex items-center gap-3">
                  <Toggle v-model="configValues[key]" />
                  <div>
                    <span class="text-sm text-slate-700 dark:text-dark-300">{{ prop.title }}</span>
                    <p v-if="prop.description" class="text-xs text-slate-400 dark:text-dark-500">{{ prop.description }}</p>
                  </div>
                </div>
              </template>
            </div>
          </div>

          <!-- Actions -->
          <div class="flex gap-3 mt-6 pt-4 border-t border-slate-100 dark:border-dark-700">
            <button
              @click="saveConfig"
              :disabled="saving"
              class="px-4 py-2 text-sm font-medium text-white bg-primary-600 hover:bg-primary-700 rounded-lg transition-colors disabled:opacity-50"
            >
              {{ $t('desktop.plugin.save') }}
            </button>
            <button
              @click="resetConfig"
              class="px-4 py-2 text-sm font-medium text-slate-700 dark:text-dark-300 bg-white dark:bg-dark-700 border border-slate-300 dark:border-dark-600 hover:bg-slate-50 dark:hover:bg-dark-600 rounded-lg transition-colors"
            >
              {{ $t('desktop.plugin.reset') }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 插件配置视图 - 自动生成配置表单
 * 根据 manifest contributes.configuration 生成 UI
 */
import { ref, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import Toggle from '@/components/Toggle.vue'
import Spinner from '@/components/Spinner.vue'
import { pluginGetInfo, pluginStorageGet, pluginStorageSet } from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import type { PluginInfo, PluginConfiguration, ConfigProperty, PluginState } from '@/plugin/types'

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const toast = useToast()

const pluginId = computed(() => route.params.id as string)
const pluginInfo = ref<PluginInfo | null>(null)
const configSchema = ref<PluginConfiguration | null>(null)
const configValues = ref<Record<string, any>>({})
const loading = ref(true)
const saving = ref(false)

/** 判断插件是否为激活状态 */
function isActivatedState(state: PluginState): boolean {
  return state.state === 'Activated'
}

/** 从 manifest defaults 构建初始值 */
function buildDefaults(schema: PluginConfiguration): Record<string, any> {
  const defaults: Record<string, any> = {}
  for (const [key, prop] of Object.entries(schema.properties)) {
    defaults[key] = prop.default !== undefined ? prop.default : getEmptyDefault(prop)
  }
  return defaults
}

function getEmptyDefault(prop: ConfigProperty): any {
  switch (prop.type) {
    case 'string': return ''
    case 'number': return 0
    case 'boolean': return false
    default: return null
  }
}

/** 加载插件信息和配置 */
async function loadConfig(): Promise<void> {
  loading.value = true
  try {
    const info = await pluginGetInfo(pluginId.value)
    if (!info) {
      router.replace('/plugins')
      return
    }
    pluginInfo.value = info

    // 插件未激活时无法配置
    if (!isActivatedState(info.state)) {
      loading.value = false
      return
    }

    // 读取配置 schema
    const schema = info.contributes.configuration || null
    configSchema.value = schema

    if (!schema) {
      loading.value = false
      return
    }

    // 从 storage 读取已保存的配置
    const saved = await pluginStorageGet(pluginId.value, 'config')
    const defaults = buildDefaults(schema)
    configValues.value = saved ? { ...defaults, ...saved } : defaults
  } catch (e: any) {
    toast.error(t('desktop.plugin.loadConfigFailed'))
  } finally {
    loading.value = false
  }
}

/** 保存配置 */
async function saveConfig(): Promise<void> {
  if (!configSchema.value) return
  saving.value = true
  try {
    await pluginStorageSet(pluginId.value, 'config', { ...configValues.value })
    toast.success(t('desktop.plugin.configSaved'))
  } catch (e: any) {
    toast.error(t('desktop.plugin.saveConfigFailed'))
  } finally {
    saving.value = false
  }
}

/** 重置为默认值 */
function resetConfig(): void {
  if (!configSchema.value) return
  configValues.value = buildDefaults(configSchema.value)
  toast.success(t('desktop.plugin.configReset'))
}

onMounted(() => {
  loadConfig()
})
</script>
