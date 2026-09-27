<script setup lang="ts">
/**
 * 供应商管理分区（票据 05 / v2 中心凭据库）：预设列表 + 内置模板新建 + 反向
 * 导入 + 应用入口
 *
 * 预设真源在插件库 `provider_preset` 表；v2 起 `api_key` 列为中心凭据库——
 * 一处配置 key、分发到各 CLI。列表/导入结果只见掩码（前 3 字符 + 长度），
 * 编辑处可直接设/清 key（明文仅在 save 命令在途），应用时经 ProviderApply
 * 面板选 stored（中心库，默认）/ inline / source / none。claude 只读卡展示
 * settings.json env 现状（掩码）与桥接文件存在性（桥接冲突在应用面板处理）。
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #a-pv（页头双动作 +
 * 安全横幅 + 预设表格 + 应用卡）。
 */
import { computed, inject, nextTick, ref, useTemplateRef, watch } from 'vue'
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
/** v2 中心凭据：编辑器输入的新 key（明文仅在 save 命令在途） */
const formKey = ref('')
/** 清空已存 key（与 formKey 互斥：输入新 key 时忽略此标志） */
const clearKey = ref(false)
/** 编辑态既有 key 掩码（新建态为 null；占位/清空按钮显示用） */
const editingKeyMask = computed(() => {
  if (editingId.value === null) return null
  return presets.value.find((p) => p.id === editingId.value)?.keyMask ?? null
})
/** 同名冲突（guest 返回 nameExists 后呈现） */
const nameExists = ref(false)

function openEditor(preset: ProviderPreset | null, templateId?: string) {
  nameExists.value = false
  formKey.value = ''
  clearKey.value = false
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
  const keyInput = formKey.value.trim()
  // apiKey 语义：输入新 key → 设置；否则 clearKey 勾选 → 清空；都没 → 保留
  const apiKey = keyInput ? keyInput : clearKey.value ? '' : undefined
  const result = await props.providers.savePreset({
    id: editingId.value ?? undefined,
    name,
    baseUrl: formBaseUrl.value.trim(),
    apiStyle: formApiStyle.value,
    models,
    apiKey,
  })
  if (result === null) return
  if (result.nameExists) {
    nameExists.value = true
    return
  }
  if (result.saved) showEditor.value = false
}

// ==================== 弹窗焦点管理 ====================

/**
 * 原实现把 `@keydown.esc` 挂在无 tabindex 的遮罩 div 上：焦点不在其子元素时
 * 事件根本不冒到那里，Esc 时常按不动。这里改为：
 * ① 打开时把焦点移入面板——落点取面板上标记的初始焦点（名称输入框），
 *    不是 DOM 里第一个可聚焦元素（否则一打开就聚焦到关闭按钮）
 * ② Tab 循环锁在面板内（focus trap）
 * ③ document 级 Esc 监听，任何焦点位置都能关
 * ④ 关闭后把焦点还给触发按钮
 */
const editorPanel = useTemplateRef<HTMLElement>('editorPanel')
const editorTrigger = useTemplateRef<HTMLElement>('editorTrigger')
const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'

function focusables(): HTMLElement[] {
  if (!editorPanel.value) return []
  return Array.from(editorPanel.value.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    // checkVisibility 过滤隐藏元素；无此 API 的环境（如测试）默认可见
    (el) => el.checkVisibility?.() ?? true,
  )
}

function onEditorKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const items = focusables()
  if (items.length === 0) return
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement as HTMLElement | null
  if (e.shiftKey && (active === first || !editorPanel.value?.contains(active))) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && active === last) {
    e.preventDefault()
    first.focus()
  }
}

function onEditorEsc() {
  if (showEditor.value) showEditor.value = false
}

watch(showEditor, async (open) => {
  if (open) {
    document.addEventListener('keydown', onEditorEsc)
    await nextTick()
    // 初始焦点：面板内 [data-autofocus]（表单首个字段），退化时取第一个可聚焦元素
    const target =
      editorPanel.value?.querySelector<HTMLElement>('[data-autofocus]') ?? focusables()[0]
    target?.focus()
  } else {
    document.removeEventListener('keydown', onEditorEsc)
    await nextTick()
    editorTrigger.value?.focus()
  }
})

/** 名称改动后作废上一轮的「同名预设」错误（否则错误会一直挂到下次打开） */
watch(formName, () => {
  nameExists.value = false
})

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
            ref="editorTrigger"
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

      <!-- key 安全横幅（常驻说明，非警告：改用信息性底色，
           把 warning 底色留给同页真警告「桥接冲突」） -->
      <div class="ah-banner ah-banner-info">
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
            <span class="ah-cli-tag ah-mono" :data-testid="`preset-keymask-${preset.name}`">
              {{ t('hub.pv.keyMask', { mask: preset.keyMask }) }}
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

      <!-- 新建/编辑表单（弹窗：Teleport 到 body；遮罩点击 / Esc / ✕ / 取消均可关闭）
           面板是 <form>：文本字段里回车 = 保存，与常规表单弹窗一致 -->
      <Teleport to="body">
        <Transition name="ah-modal">
          <div
            v-if="showEditor"
            class="ah-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="pv-editor-title"
            @click.self="showEditor = false"
          >
            <form
              ref="editorPanel"
              class="ah-modal-panel ah-card"
              tabindex="-1"
              data-testid="preset-editor"
              @submit.prevent="save"
              @keydown="onEditorKeydown"
            >
              <div class="ah-modal-head">
                <h2 id="pv-editor-title" class="ah-modal-title">
                  {{ editingId === null ? t('hub.pv.editor.titleNew') : t('hub.pv.editor.titleEdit') }}
                </h2>
                <button
                  type="button"
                  class="ah-modal-close"
                  :aria-label="t('hub.pv.editor.close')"
                  :title="t('hub.pv.editor.close')"
                  data-testid="preset-editor-close"
                  @click="showEditor = false"
                >
                  ✕
                </button>
              </div>

              <div class="ah-modal-body">
                <!-- 模板快选（仅新建态） -->
                <div v-if="editingId === null" class="ah-pv-field">
                  <span id="pv-template-label" class="ah-pv-label">{{ t('hub.pv.editor.template') }}</span>
                  <div class="ah-pv-targets" role="group" aria-labelledby="pv-template-label">
                    <button
                      v-for="tpl in PROVIDER_TEMPLATES"
                      :key="tpl.id"
                      type="button"
                      class="ah-cli-tag ah-pv-target"
                      :data-testid="`preset-template-${tpl.id}`"
                      @click="pickTemplate(tpl.id)"
                    >
                      {{ tpl.id === 'custom' ? t('hub.pv.style.custom') : tpl.name }}
                    </button>
                  </div>
                </div>

                <!-- 名称（唯一必填项） -->
                <div class="ah-pv-field">
                  <label class="ah-pv-label" for="pv-name">
                    {{ t('hub.pv.editor.name') }}
                    <span
                      class="ah-pv-req"
                      :aria-label="t('hub.pv.editor.required')"
                      :title="t('hub.pv.editor.required')"
                    >*</span>
                  </label>
                  <input
                    id="pv-name"
                    v-model="formName"
                    class="ah-input ah-pv-input"
                    type="text"
                    spellcheck="false"
                    data-autofocus
                    aria-required="true"
                    :aria-invalid="nameExists ? 'true' : undefined"
                    :aria-describedby="nameExists ? 'pv-name-error' : undefined"
                    data-testid="preset-name"
                  />
                  <div v-if="nameExists" id="pv-name-error" class="ah-cli-error" role="alert">
                    {{ t('hub.pv.editor.nameExists') }}
                  </div>
                </div>

                <div class="ah-pv-field">
                  <label class="ah-pv-label" for="pv-baseurl">{{ t('hub.pv.editor.baseUrl') }}</label>
                  <input
                    id="pv-baseurl"
                    v-model="formBaseUrl"
                    class="ah-input ah-pv-input ah-mono"
                    type="text"
                    spellcheck="false"
                    data-testid="preset-baseurl"
                  />
                </div>

                <div class="ah-pv-field">
                  <span id="pv-style-label" class="ah-pv-label">{{ t('hub.pv.editor.apiStyle') }}</span>
                  <div class="ah-pv-targets" role="group" aria-labelledby="pv-style-label">
                    <button
                      v-for="style in API_STYLES"
                      :key="style"
                      type="button"
                      class="ah-cli-tag ah-pv-target"
                      :class="{ active: formApiStyle === style }"
                      :aria-pressed="formApiStyle === style"
                      :data-testid="`preset-style-${style}`"
                      @click="formApiStyle = style"
                    >
                      {{ styleLabel(style) }}
                    </button>
                  </div>
                </div>

                <div class="ah-pv-field">
                  <label class="ah-pv-label" for="pv-models">{{ t('hub.pv.editor.models') }}</label>
                  <textarea
                    id="pv-models"
                    v-model="formModels"
                    class="ah-sk-textarea ah-pv-models ah-mono"
                    spellcheck="false"
                    data-testid="preset-models"
                  ></textarea>
                </div>

                <!-- v2 中心凭据：新 key 输入 / 清空切换（明文仅在 save 命令在途） -->
                <div class="ah-pv-field">
                  <label class="ah-pv-label" for="pv-key">{{ t('hub.pv.editor.key') }}</label>
                  <div class="ah-pv-keyrow">
                    <input
                      id="pv-key"
                      v-model="formKey"
                      class="ah-input ah-pv-input ah-mono"
                      type="password"
                      autocomplete="new-password"
                      spellcheck="false"
                      :placeholder="editingKeyMask ? t('hub.pv.editor.keyPlaceholder', { mask: editingKeyMask }) : t('hub.pv.editor.keyNew')"
                      data-testid="preset-key"
                    />
                    <button
                      v-if="editingKeyMask"
                      type="button"
                      class="ah-btn ah-btn-ghost ah-btn-sm"
                      :class="{ 'ah-btn-warn': clearKey }"
                      :aria-pressed="clearKey"
                      :disabled="busy"
                      data-testid="preset-key-clear"
                      @click="clearKey = !clearKey"
                    >
                      {{ clearKey ? t('hub.pv.editor.keyKeep') : t('hub.pv.editor.keyClear') }}
                    </button>
                  </div>
                  <div class="ah-inst-hint">{{ t('hub.pv.editor.keyHint') }}</div>
                </div>
              </div>

              <div class="ah-modal-foot">
                <button
                  type="button"
                  class="ah-btn ah-btn-ghost"
                  data-testid="preset-cancel"
                  @click="showEditor = false"
                >
                  {{ t('hub.pv.editor.cancel') }}
                </button>
                <button
                  type="submit"
                  class="ah-btn ah-btn-primary"
                  :disabled="busy || !formName.trim()"
                  data-testid="preset-save"
                >
                  {{ t('hub.pv.editor.save') }}
                </button>
              </div>
            </form>
          </div>
        </Transition>
      </Teleport>
    </template>
  </div>
</template>
