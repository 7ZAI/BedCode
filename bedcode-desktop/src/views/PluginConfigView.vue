<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左返回+名称，右保存/重置 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5 min-w-0">
        <button
          class="h-7 w-7 rounded-[6px] border border-[var(--border)] flex items-center justify-center text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0"
          :title="$t('desktop.plugin.title')"
          @click="goBack"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)] truncate">
          {{ pluginInfo ? pluginInfo.name : '...' }}
        </h1>
        <span v-if="pluginInfo" class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0">v{{ pluginInfo.version }}</span>
      </div>
      <div v-if="configSchema && pluginInfo && isActivatedState(pluginInfo.state)" class="flex items-center gap-2">
        <PluginPageToolbar target="plugin-config" />
        <button class="wb-btn-ghost" @click="resetConfig">
          {{ $t('desktop.plugin.reset') }}
        </button>
        <button class="wb-btn-primary" :disabled="saving" @click="saveConfig">
          {{ $t('desktop.plugin.save') }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto p-5">
      <div class="max-w-3xl mx-auto">
        <!-- ==================== 加载态：骨架 ==================== -->
        <div v-if="loading" class="space-y-6">
          <div v-for="i in 2" :key="i">
            <div class="h-3 w-40 rounded animate-pulse bg-[var(--bg-hover)] mb-2"></div>
            <div class="h-24 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"></div>
          </div>
        </div>

        <!-- ==================== 未激活 ==================== -->
        <div v-else-if="!pluginInfo || !isActivatedState(pluginInfo.state)" class="py-16 text-center text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)]">
          {{ $t('desktop.plugin.pluginNotActivated') }}
        </div>

        <template v-else>
          <!-- ---------- SECTION: CONFIGURATION ---------- -->
          <section v-if="configSchema" class="mb-6">
            <h2 class="wb-section-title">{{ $t('desktop.plugin.config').toUpperCase() }}</h2>
            <div v-if="Object.keys(configSchema.properties).length === 0" class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-4 py-6 text-center text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)]">
              {{ pluginInfo.name }} — {{ $t('desktop.plugin.noConfigAvailable') }}
            </div>
            <div v-else class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
              <div
                v-for="(prop, key) in configSchema.properties"
                :key="key"
                class="px-4 py-3"
              >
                <!-- boolean：行内开关 -->
                <div v-if="prop.type === 'boolean'" class="flex items-center justify-between min-h-9">
                  <div>
                    <div class="text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{ prop.title }}</div>
                    <div v-if="prop.description" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-0.5">{{ prop.description }}</div>
                  </div>
                  <button
                    class="relative w-10 h-5 rounded-[4px] border transition-colors shrink-0"
                    :class="configValues[key] ? 'bg-[var(--color-primary)] border-[var(--color-primary)]' : 'bg-[var(--bg-page)] border-[var(--border-strong)]'"
                    @click="configValues[key] = !configValues[key]"
                  >
                    <span
                      class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                      :class="configValues[key] ? 'left-[22px] bg-[var(--color-primary-contrast)]' : 'left-[3px] bg-[var(--border-strong)]'"
                    ></span>
                  </button>
                </div>

                <!-- 其余类型：label 上 / 控件下 -->
                <template v-else>
                  <div class="flex items-baseline gap-2 mb-1.5">
                    <span class="text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{ prop.title }}</span>
                    <span v-if="prop.description" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">{{ prop.description }}</span>
                  </div>
                  <select
                    v-if="prop.type === 'string' && prop.enum"
                    v-model="configValues[key]"
                    class="w-full max-w-xs h-8 px-2 wb-mono rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                  >
                    <option v-for="opt in prop.enum" :key="opt" :value="opt">{{ opt }}</option>
                  </select>
                  <input
                    v-else-if="prop.type === 'string'"
                    v-model="configValues[key]"
                    type="text"
                    class="w-full max-w-md h-8 px-2.5 wb-mono rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                  />
                  <input
                    v-else-if="prop.type === 'number'"
                    v-model.number="configValues[key]"
                    type="number"
                    class="w-32 h-8 px-2.5 wb-mono rounded-[6px] border border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                  />
                </template>
              </div>
            </div>
          </section>

          <div v-else class="mb-6 bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-4 py-6 text-center text-[calc(12.5px*var(--ui-scale))] text-[var(--text-secondary)]">
            {{ pluginInfo.name }} — {{ $t('desktop.plugin.noConfigAvailable') }}
          </div>

          <!-- ---------- SECTION: INFO（技术值 mono） ---------- -->
          <section>
            <h2 class="wb-section-title">INFO</h2>
            <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]">
              <div class="flex items-center justify-between px-4 h-10">
                <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">ID</span>
                <span class="wb-mono text-[var(--text-primary)]">{{ pluginInfo.id }}</span>
              </div>
              <div v-if="pluginInfo.author" class="flex items-center justify-between px-4 h-10">
                <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)]">Author</span>
                <span class="text-[calc(12.5px*var(--ui-scale))] text-[var(--text-primary)]">{{ pluginInfo.author }}</span>
              </div>
              <div class="flex items-center justify-between px-4 min-h-10 py-2 gap-4">
                <span class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] shrink-0">{{ $t('desktop.plugin.copyPath') }}</span>
                <div class="flex items-center gap-2 min-w-0">
                  <span class="wb-mono text-[var(--text-primary)] truncate">{{ pluginInfo.extensionPath }}</span>
                  <button
                    class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:text-[var(--text-primary)] underline-offset-2 hover:underline shrink-0"
                    @click="copyPath(pluginInfo.extensionPath)"
                  >
                    {{ $t('desktop.plugin.copyPath') }}
                  </button>
                </div>
              </div>
            </div>
          </section>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 插件配置视图 — 自动生成配置表单
 * Warm Workbench 风格：工具栏页头右置保存，CONFIG/INFO 分区，技术值 mono
 */
import { ref, onMounted, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { pluginGetInfo, pluginStorageGet, pluginStorageSet } from '@/plugin/commands'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import { useToast } from '@/composables/useToast'
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
      router.replace({ path: '/plugins', query: { ...route.query } })
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

/** 复制扩展路径到剪贴板 */
async function copyPath(path: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(path)
    toast.success(t('desktop.plugin.pathCopied'))
  } catch {
    toast.error(t('desktop.plugin.copyFailed'))
  }
}

/** 返回插件列表 — 携带查询参数 */
function goBack(): void {
  router.push({ path: '/plugins', query: { ...route.query } })
}

onMounted(() => {
  loadConfig()
})
</script>
