<script setup lang="ts">
/**
 * 供应商管理分区（票据 05）：预设列表 + 内置模板新建 + 反向导入 + 应用入口
 *
 * 预设真源在插件库 `provider_preset` 表（无 key 列）；key 全链路不落 hub——
 * 列表/导入结果只见掩码（前 3 字符 + 长度），应用时经 ProviderApply 面板
 * 现场输入或内存直拷。claude 只读卡展示 settings.json env 现状（掩码）与
 * 桥接文件存在性（桥接冲突在应用面板处理）。
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #a-pv（页头双动作 +
 * 安全横幅 + 预设表格 + 应用卡）。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, ProviderPreset, ProvidersDomainState } from '../types'
import type { UseProvidersReturn } from '../composables/useProviders'
import { PROVIDER_TEMPLATES, sourceFromNotes } from '../utils/providers'
import ProviderApply from './ProviderApply.vue'

const props = defineProps<{
  detection: AgentHubState | null
  providers: UseProvidersReturn
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const state = computed<ProvidersDomainState | null>(() => props.providers.state.value)
const presets = computed<ProviderPreset[]>(() => state.value?.presets ?? [])

/** fs 类动作依赖目录授权；拒绝时置灰降级（与概览/安装页同一横幅语义） */
const authGranted = computed(() => props.detection?.authGranted ?? false)
const busy = computed(
  () => props.providers.importing.value || props.providers.saving.value || props.providers.applying.value,
)

const API_STYLES = ['openai', 'anthropic', 'gemini', 'custom'] as const

function styleLabel(style: string): string {
  return t(`hub.pv.style.${style}`)
}

// ==================== 新建 / 编辑 ====================

const showEditor = ref(false)
const editingId = ref<number | null>(null)
const formName = ref('')
const formBaseUrl = ref('')
const formApiStyle = ref<string>('openai')
const formModels = ref('')
/** 同名冲突（guest 返回 nameExists 后呈现） */
const nameExists = ref(false)

function openEditor(preset: ProviderPreset | null, templateId?: string) {
  nameExists.value = false
  if (preset) {
    editingId.value = preset.id
    formName.value = preset.name
    formBaseUrl.value = preset.baseUrl
    formApiStyle.value = preset.apiStyle
    formModels.value = preset.models.join('\n')
  } else {
    editingId.value = null
    const tpl = PROVIDER_TEMPLATES.find((x) => x.id === templateId) ?? null
    formName.value = tpl?.name ?? ''
    formBaseUrl.value = tpl?.baseUrl ?? ''
    formApiStyle.value = tpl?.apiStyle ?? 'openai'
    formModels.value = (tpl?.models ?? []).join('\n')
  }
  showEditor.value = true
}

function pickTemplate(id: string) {
  // 模板只在新建态提供（编辑态保留原值）
  if (editingId.value !== null) return
  openEditor(null, id)
}

async function save() {
  const name = formName.value.trim()
  if (!name) return
  const models = formModels.value
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean)
  const result = await props.providers.savePreset({
    id: editingId.value ?? undefined,
    name,
    baseUrl: formBaseUrl.value.trim(),
    apiStyle: formApiStyle.value,
    models,
  })
  if (result === null) return
  if (result.nameExists) {
    nameExists.value = true
    return
  }
  if (result.saved) showEditor.value = false
}

// ==================== 删除（两击确认） ====================

const deleteArmId = ref<number | null>(null)

async function remove(preset: ProviderPreset) {
  if (deleteArmId.value !== preset.id) {
    deleteArmId.value = preset.id
    return
  }
  deleteArmId.value = null
  await props.providers.deletePreset(preset.id)
}

// ==================== 反向导入 ====================

const importResultShown = ref(false)

async function importProviders() {
  importResultShown.value = true
  await props.providers.importProviders()
}

const importLast = computed(() => state.value?.import.last ?? null)

// ==================== 应用 ====================

const applyingPreset = ref<ProviderPreset | null>(null)

function sourceTag(preset: ProviderPreset): string {
  const source = sourceFromNotes(preset.notes)
  return source ? t('hub.pv.source.imported', { source: source.cli }) : t('hub.pv.source.manual')
}
</script>

<template>
  <div class="ah-pv">
    <div v-if="!authGranted" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
    </div>

    <!-- 应用面板（替换列表视图） -->
    <ProviderApply
      v-if="applyingPreset"
      :providers="props.providers"
      :preset="applyingPreset"
      @close="applyingPreset = null"
    />

    <template v-else>
      <div class="ah-inst-head">
        <span class="ah-section-title">{{ t('hub.tab.providers') }}</span>
        <span class="ah-speed-actions-btns">
          <button
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="busy || !authGranted"
            data-testid="import-providers"
            @click="importProviders"
          >
            {{ props.providers.importing.value ? t('hub.pv.importing') : t('hub.pv.import') }}
          </button>
          <button
            type="button"
            class="ah-btn ah-btn-primary ah-btn-sm"
            :disabled="busy"
            data-testid="new-preset"
            @click="openEditor(null)"
          >
            {{ t('hub.pv.new') }}
          </button>
        </span>
      </div>

      <!-- key 安全横幅（原型 #a-pv 🔐） -->
      <div class="ah-banner">
        <span class="ah-banner-ic">🔐</span>
        <span class="ah-banner-text">{{ t('hub.pv.secure') }}</span>
      </div>

      <!-- 导入结果（guest 状态回流：掩码 keys） -->
      <div v-if="importResultShown && importLast" class="ah-card ah-pv-import" data-testid="import-result">
        <div v-if="importLast.created.length > 0" class="ah-sk-result">
          {{ t('hub.pv.importDone', { n: importLast.created.length }) }}
          <span class="ah-pv-import-names">{{ importLast.created.join(', ') }}</span>
        </div>
        <div v-if="importLast.skipped.length > 0" class="ah-inst-hint ah-pv-import-line">
          {{ t('hub.pv.importSkipped', { names: importLast.skipped.join(', ') }) }}
        </div>
        <div v-if="importLast.created.length === 0 && importLast.skipped.length === 0" class="ah-inst-hint">
          {{ t('hub.pv.importNone') }}
        </div>
        <div v-for="(mask, name) in importLast.keys" :key="name" class="ah-inst-hint ah-mono ah-pv-import-line">
          {{ t('hub.pv.keyMask', { name, mask }) }}
        </div>
      </div>

      <!-- claude 只读视图（env 掩码 + 桥接存在性） -->
      <div class="ah-card" data-testid="claude-view">
        <div class="ah-section-title">{{ t('hub.pv.claude.title') }}</div>
        <template v-if="state?.claude.env.baseUrl || state?.claude.env.model || state?.claude.env.authTokenMask">
          <div class="ah-env-rows ah-pv-claude-rows">
            <span v-if="state?.claude.env.baseUrl" class="ah-env-row">
              <span class="ah-env-label">{{ t('hub.pv.claude.baseUrl') }}</span>
              <span class="ah-env-value ah-mono">{{ state?.claude.env.baseUrl }}</span>
            </span>
            <span v-if="state?.claude.env.model" class="ah-env-row">
              <span class="ah-env-label">{{ t('hub.pv.claude.model') }}</span>
              <span class="ah-env-value ah-mono">{{ state?.claude.env.model }}</span>
            </span>
            <span v-if="state?.claude.env.authTokenMask" class="ah-env-row">
              <span class="ah-env-label">{{ t('hub.pv.claude.token') }}</span>
              <span class="ah-env-value ah-mono">{{ state?.claude.env.authTokenMask }}</span>
            </span>
          </div>
        </template>
        <div v-else class="ah-inst-hint ah-pv-claude-rows">{{ t('hub.pv.claude.none') }}</div>
        <div
          v-if="state?.claude.bridge.providerConfigSh || state?.claude.bridge.anthropicBridgeMjs"
          class="ah-cli-dual"
        >
          ⚠
          {{
            t('hub.pv.claude.bridge', {
              files: [
                state?.claude.bridge.providerConfigSh ? 'provider-config.sh' : null,
                state?.claude.bridge.anthropicBridgeMjs ? 'anthropic-bridge.mjs' : null,
              ]
                .filter(Boolean)
                .join(' / '),
            })
          }}
        </div>
      </div>

      <!-- 空态 -->
      <div v-if="presets.length === 0" class="ah-card ah-sk-empty">
        <div class="ah-section-title">{{ t('hub.pv.empty') }}</div>
        <div class="ah-inst-hint ah-sk-empty-hint">{{ t('hub.pv.emptyHint') }}</div>
      </div>

      <!-- 预设行（列语义：预设+模型 | 来源 | 动作，窄面板自动换行） -->
      <div v-if="presets.length > 0" class="ah-card ah-sk-table-card">
        <div v-for="preset in presets" :key="preset.id" class="ah-sk-row" :data-testid="`preset-row-${preset.name}`">
          <div class="ah-sk-row-main">
            <span class="ah-sk-row-name">
              <span class="ah-sk-row-name-text">{{ preset.name }}</span>
              <span class="ah-cli-tag">{{ styleLabel(preset.apiStyle) }}</span>
            </span>
            <span class="ah-sk-row-dir ah-mono">{{ preset.baseUrl || '—' }}</span>
            <span class="ah-sk-row-desc ah-mono">
              {{ t('hub.pv.models', { n: preset.models.length }) }}<template v-if="preset.models.length > 0">: {{ preset.models.slice(0, 4).join(' · ') }}<template v-if="preset.models.length > 4"> …</template></template>
            </span>
          </div>
          <div class="ah-sk-row-dist">
            <span class="ah-cli-tag" :class="sourceFromNotes(preset.notes) ? 'ok' : ''">
              <span class="ah-cli-dot"></span>{{ sourceTag(preset) }}
            </span>
          </div>
          <div class="ah-sk-row-actions">
            <button
              type="button"
              class="ah-btn ah-btn-primary ah-btn-sm"
              :disabled="busy || !authGranted"
              :data-testid="`apply-${preset.name}`"
              @click="applyingPreset = preset"
            >
              {{ t('hub.pv.apply') }}
            </button>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :disabled="busy"
              :data-testid="`edit-${preset.name}`"
              @click="openEditor(preset)"
            >
              {{ t('hub.pv.edit') }}
            </button>
            <button
              type="button"
              class="ah-btn ah-btn-ghost ah-btn-sm"
              :class="{ 'ah-btn-warn': deleteArmId === preset.id }"
              :disabled="busy"
              :data-testid="`delete-${preset.name}`"
              @click="remove(preset)"
            >
              {{ deleteArmId === preset.id ? t('hub.pv.deleteConfirm', { name: preset.name }) : t('hub.pv.delete') }}
            </button>
          </div>
        </div>
      </div>

      <!-- 新建/编辑表单 -->
      <div v-if="showEditor" class="ah-card ah-pv-editor" data-testid="preset-editor">
        <div class="ah-inst-head">
          <span class="ah-section-title">
            {{ editingId === null ? t('hub.pv.editor.titleNew') : t('hub.pv.editor.titleEdit') }}
          </span>
        </div>

        <!-- 模板快选（仅新建态） -->
        <div v-if="editingId === null" class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.editor.template') }}</span>
          <span class="ah-pv-targets">
            <button
              v-for="tpl in PROVIDER_TEMPLATES"
              :key="tpl.id"
              type="button"
              class="ah-cli-tag ah-pv-target"
              @click="pickTemplate(tpl.id)"
            >
              {{ tpl.id === 'custom' ? t('hub.pv.style.custom') : tpl.name }}
            </button>
          </span>
        </div>

        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.editor.name') }}</span>
          <input v-model="formName" class="ah-sk-url ah-pv-input" type="text" spellcheck="false" data-testid="preset-name" />
        </div>
        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.editor.baseUrl') }}</span>
          <input v-model="formBaseUrl" class="ah-sk-url ah-pv-input ah-mono" type="text" spellcheck="false" data-testid="preset-baseurl" />
        </div>
        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.editor.apiStyle') }}</span>
          <span class="ah-pv-targets">
            <button
              v-for="style in API_STYLES"
              :key="style"
              type="button"
              class="ah-cli-tag ah-pv-target"
              :class="{ active: formApiStyle === style }"
              @click="formApiStyle = style"
            >
              {{ styleLabel(style) }}
            </button>
          </span>
        </div>
        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.editor.models') }}</span>
          <textarea v-model="formModels" class="ah-sk-textarea ah-pv-models ah-mono" spellcheck="false" data-testid="preset-models"></textarea>
        </div>

        <div v-if="nameExists" class="ah-cli-error">{{ t('hub.pv.editor.nameExists') }}</div>

        <div class="ah-pv-apply-actions">
          <button
            type="button"
            class="ah-btn ah-btn-primary ah-btn-sm"
            :disabled="busy || !formName.trim()"
            data-testid="preset-save"
            @click="save"
          >
            {{ t('hub.pv.editor.save') }}
          </button>
          <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="showEditor = false">
            {{ t('hub.pv.editor.cancel') }}
          </button>
        </div>
      </div>
    </template>
  </div>
</template>
