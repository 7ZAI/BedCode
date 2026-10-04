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
import { toast } from 'vue-sonner'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, ProviderPreset, ProvidersDomainState } from '../types'
import type { UseProvidersReturn } from '../composables/useProviders'
import { PROVIDER_TEMPLATES, sourceFromNotes } from '../utils/providers'
import { defaultModelsUrl, mergeModelIds, modelsToText, parseModelsText } from '../utils/providers'
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
/** 模型查询 URL（可空 = 手动输入模型列表；查询与手动互不依赖） */
const formModelsUrl = ref('')
/** 查询到的模型 id（未加入模型列表的候选；点 chip 逐个加入） */
const fetchedModels = ref<string[]>([])
/** 查询失败提示（空串 = 无失败；不展示 guest 原文，只给可行动文案） */
const modelsQueryError = ref(false)
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

/**
 * 表单内容指纹：openEditor 时定格一份，用于判定「用户是否已填内容」。
 * 含 apiStyle / key——光改方言或填了 key 也算已填，同样不能被模板无声覆盖。
 */
function formSignature(): string {
  return JSON.stringify([
    formName.value,
    formBaseUrl.value,
    formApiStyle.value,
    formModels.value,
    formModelsUrl.value,
    formKey.value,
    clearKey.value,
  ])
}

/** openEditor 时的指纹基准（普通变量：只在 openEditor / 应用模板时重写，无需响应式） */
let formBaseline = formSignature()

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
    formModelsUrl.value = preset.modelsUrl ?? ''
  } else {
    editingId.value = null
    const tpl = PROVIDER_TEMPLATES.find((x) => x.id === templateId) ?? null
    formName.value = tpl?.name ?? ''
    formBaseUrl.value = tpl?.baseUrl ?? ''
    formApiStyle.value = tpl?.apiStyle ?? 'openai'
    formModels.value = (tpl?.models ?? []).join('\n')
    formModelsUrl.value = tpl ? defaultModelsUrl(tpl.baseUrl) : ''
  }
  fetchedModels.value = []
  modelsQueryError.value = false
  pendingTemplateId.value = null
  formBaseline = formSignature()
  showEditor.value = true
}

/** 用户是否已改动过表单（与打开时/应用模板后的基准比对） */
const formTouched = computed(() => formSignature() !== formBaseline)

/** 待确认覆盖的模板 id（非空 = 覆盖确认条展示中） */
const pendingTemplateId = ref<string | null>(null)

function applyTemplate(id: string) {
  const tpl = PROVIDER_TEMPLATES.find((x) => x.id === id)
  if (!tpl) return
  pendingTemplateId.value = null
  nameExists.value = false
  formKey.value = ''
  clearKey.value = false
  formName.value = tpl.name
  formBaseUrl.value = tpl.baseUrl
  formApiStyle.value = tpl.apiStyle
  formModels.value = tpl.models.join('\n')
  formModelsUrl.value = defaultModelsUrl(tpl.baseUrl)
  fetchedModels.value = []
  modelsQueryError.value = false
  formBaseline = formSignature()
}

/**
 * 模板快选（仅新建态）
 *
 * 模板语义是「从头开始」，直接 openEditor 会静默抹掉用户刚输入的内容。
 * 分流：表单未被改动 → 直接填满（零摩擦）；已改动 → 先弹覆盖确认条。
 * 不采用「只填空字段」：那会拼出「用户 URL + 模板模型」的自相矛盾组合，
 * 错得隐蔽；宁可多一次点击。
 */
function pickTemplate(id: string) {
  // 模板只在新建态提供（编辑态保留原值）
  if (editingId.value !== null) return
  if (formTouched.value) {
    pendingTemplateId.value = id
    return
  }
  applyTemplate(id)
}

async function save() {
  // busy 守卫：保存按钮 disabled 只是第一道防线，回车/快速连点仍可能重入。
  // 这里必须先拦——否则 composable 返回 busy 会被当成「保存失败」误报。
  if (busy.value) return
  const name = formName.value.trim()
  if (!name) return
  const models = parseModelsText(formModels.value)
  const keyInput = formKey.value.trim()
  // apiKey 语义：输入新 key → 设置；否则 clearKey 勾选 → 清空；都没 → 保留
  const apiKey = keyInput ? keyInput : clearKey.value ? '' : undefined
  // 新建/编辑在关窗前定格：关窗后不能再拿它拼 toast 文案
  const wasEditing = editingId.value !== null
  const res = await props.providers.savePreset({
    id: editingId.value ?? undefined,
    name,
    baseUrl: formBaseUrl.value.trim(),
    apiStyle: formApiStyle.value,
    models,
    modelsUrl: formModelsUrl.value.trim(),
    apiKey,
  })
  // 被忽略（并发中）：静默，调用方本就在做同一件事
  if (res.status === 'busy') return
  if (res.status === 'error') {
    toast.error(t('hub.pv.editor.saveFailed'))
    return
  }
  const result = res.data
  if (!result) {
    toast.error(t('hub.pv.editor.saveFailed'))
    return
  }
  if (result.nameExists) {
    nameExists.value = true
    return
  }
  if (!result.saved) {
    // 既未保存也无同名冲突 = guest 回执异常，不装作无事发生
    toast.error(t('hub.pv.editor.saveFailed'))
    return
  }
  showEditor.value = false
  toast.success(t(wasEditing ? 'hub.pv.toast.updated' : 'hub.pv.toast.created'))
}

// ==================== 模型列表查询（可选 URL，与手动输入互补） ====================

/** 填入由 baseUrl 派生的默认查询 URL（用户可再改成网关实际路径） */
function fillDerivedModelsUrl() {
  formModelsUrl.value = defaultModelsUrl(formBaseUrl.value)
}

/**
 * 查询模型列表（guest 代发 GET）
 *
 * 失败**不留旧候选**：沿用上一次的查询结果会被误当成本次结果（列表看着还在，
 * 其实早过期）。手动输入路径不受影响——查询只是候选来源。
 */
async function queryModels() {
  if (busy.value || props.providers.fetchingModels.value) return
  const url = formModelsUrl.value.trim() || defaultModelsUrl(formBaseUrl.value)
  if (!url) {
    modelsQueryError.value = true
    return
  }
  if (!/^https?:\/\//i.test(url)) {
    // 非 http(s)（如 file://）直接拦在前端，guest 也会拒
    modelsQueryError.value = true
    return
  }
  // 拒绝带 userinfo 的 URL（https://user:key@host/...）：凭据嵌 URL 既会在
  // 失败日志中被回显，也会被当作普通 URL 保存进预设（A-08，2026-10-04 OCR）
  if (/^https?:\/\/[^/@]+@/i.test(url)) {
    modelsQueryError.value = true
    return
  }
  formModelsUrl.value = url
  fetchedModels.value = []
  modelsQueryError.value = false
  const res = await props.providers.fetchModels({
    url,
    // 编辑态优先用中心库已存 key（明文不进前端；新建态用手填的 key）
    presetId: editingId.value ?? undefined,
    apiKey: formKey.value.trim() || undefined,
  })
  if (res.status === 'busy') return
  if (res.status === 'error') {
    // 失败只能记状态/类别，不回显 guest 原文（A-07，2026-10-04 OCR）：
    // guest 错误可能带 URL——若 URL 含用户凭据（query/嵌入），进日志即泄露
    console.error('[Agent Hub] fetch models failed (status=error)')
    modelsQueryError.value = true
    return
  }
  fetchedModels.value = res.data?.models ?? []
}

/** 模型清单解析结果（同一文本域 O(行) 解析只做一次，复用给 chips 与全加） */
const parsedModels = computed(() => parseModelsText(formModels.value))

/** 候选 chip → 加入/移出模型列表（已加入的 chip 呈激活态，可点掉） */
function toggleFetchedModel(id: string) {
  const next = parsedModels.value.includes(id)
    ? parsedModels.value.filter((m) => m !== id)
    : [...parsedModels.value, id]
  formModels.value = modelsToText(next)
}

/** 候选是否已在模型列表里（chip 激活态）——O(1) 查 Set，不再每 chip 每渲染
 * 重新解析整个文本域（A-10，2026-10-04 OCR：几百个候选时 O(chips×lines)） */
const parsedModelSet = computed(() => new Set(parsedModels.value))
function modelInList(id: string): boolean {
  return parsedModelSet.value.has(id)
}

/** 把尚未加入的候选全部追加（已加入的不重复追加，手动条目保留） */
function addAllFetched() {
  formModels.value = modelsToText(
    mergeModelIds(parsedModels.value, fetchedModels.value),
  )
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

/**
 * document 级 Esc 监听：面板 Teleport 到 body，事件在 document 上兜底关闭。
 * 必须先按 key 过滤——此监听对**所有** keydown 生效，漏判就成了「敲任意键
 * 弹窗即关」（历史 bug：输入框里一打字就被关掉）。
 * isComposing 同样放过：组合输入中 Esc 是取消候选词，不是关弹窗。
 */
function onEditorEsc(e: KeyboardEvent) {
  if (e.key !== 'Escape') return
  if (e.isComposing) return
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

/**
 * 改 URL / 基址后作废上一轮的候选模型（2026-10-04 OCR A-05）
 *
 * fetchedModels 此前只在「下一次查询 / 开编辑器 / 关编辑器」时清空：成功查询后
 * 改 models URL 或换预设（A→B），旧 A 的 chips 仍显示，`addAllFetched` 把
 * A 模型并进现 B 目标的列表。URL 是候选的查询键，变了就是另一批候选。
 * 手动输入路径不受影响（查询只是候选来源）。
 */
watch([formModelsUrl, formBaseUrl], () => {
  fetchedModels.value = []
  modelsQueryError.value = false
})

// ==================== 删除（两击确认） ====================

const deleteArmId = ref<number | null>(null)

async function remove(preset: ProviderPreset) {
  if (deleteArmId.value !== preset.id) {
    deleteArmId.value = preset.id
    return
  }
  deleteArmId.value = null
  const res = await props.providers.deletePreset(preset.id)
  if (res.status === 'busy') return
  if (res.status === 'error') {
    toast.error(t('hub.pv.deleteFailed'))
    return
  }
  toast.success(t('hub.pv.toast.deleted'))
}

// ==================== 反向导入 ====================

const importResultShown = ref(false)

async function importProviders() {
  if (busy.value) return
  // 先置 false：失败时不能沿用上一次的回执卡片（会把旧结果当成本次导入）
  importResultShown.value = false
  const res = await props.providers.importProviders()
  if (res.status === 'busy') return
  if (res.status === 'error') {
    toast.error(t('hub.pv.importFailed'))
    return
  }
  importResultShown.value = true
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
              <!-- 0 模型的预设点名：应用到 pi / opencode 会被拒（空供应商在目标 CLI 里不可见） -->
              <span
                v-if="preset.models.length === 0"
                class="ah-cli-tag warn"
                :data-testid="`preset-no-models-${preset.name}`"
              >{{ t('hub.pv.modelsMissing') }}</span>
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
                  <!-- 覆盖确认：已填内容时点模板不无声抹除，先问一句 -->
                  <div v-if="pendingTemplateId" class="ah-banner ah-sk-confirm" data-testid="template-confirm">
                    <span class="ah-banner-ic">⚠</span>
                    <span class="ah-banner-text">{{ t('hub.pv.editor.templateOverwrite') }}</span>
                    <button
                      type="button"
                      class="ah-btn ah-btn-warn ah-btn-ghost ah-btn-sm"
                      data-testid="template-overwrite-confirm"
                      @click="applyTemplate(pendingTemplateId)"
                    >
                      {{ t('hub.pv.editor.templateOverwriteConfirm') }}
                    </button>
                    <button
                      type="button"
                      class="ah-btn ah-btn-ghost ah-btn-sm"
                      data-testid="template-overwrite-cancel"
                      @click="pendingTemplateId = null"
                    >
                      {{ t('hub.pv.editor.cancel') }}
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

                <!-- 模型查询 URL（可选）：查到的候选点 chip 加入上方列表；
                     查询失败不影响手动输入（两条路径互补） -->
                <div class="ah-pv-field">
                  <label class="ah-pv-label" for="pv-models-url">{{ t('hub.pv.editor.modelsUrl') }}</label>
                  <div class="ah-pv-keyrow">
                    <input
                      id="pv-models-url"
                      v-model="formModelsUrl"
                      class="ah-input ah-pv-input ah-mono"
                      type="text"
                      spellcheck="false"
                      :placeholder="defaultModelsUrl(formBaseUrl)"
                      data-testid="preset-models-url"
                    />
                    <button
                      type="button"
                      class="ah-btn ah-btn-ghost ah-btn-sm"
                      :disabled="busy || props.providers.fetchingModels.value"
                      data-testid="preset-models-url-derive"
                      @click="fillDerivedModelsUrl"
                    >
                      {{ t('hub.pv.editor.modelsUrlDerive') }}
                    </button>
                    <button
                      type="button"
                      class="ah-btn ah-btn-ghost ah-btn-sm"
                      :disabled="busy || props.providers.fetchingModels.value"
                      data-testid="preset-models-fetch"
                      @click="queryModels"
                    >
                      {{ props.providers.fetchingModels.value ? t('hub.pv.editor.modelsFetching') : t('hub.pv.editor.modelsFetch') }}
                    </button>
                  </div>
                  <div class="ah-inst-hint">{{ t('hub.pv.editor.modelsUrlHint') }}</div>
                  <div v-if="modelsQueryError" class="ah-cli-error" role="alert" data-testid="preset-models-fetch-error">
                    {{ t('hub.pv.editor.modelsFetchFailed') }}
                  </div>
                  <div v-if="fetchedModels.length > 0" class="ah-pv-targets ah-pv-modelchips" data-testid="preset-models-candidates">
                    <button
                      v-for="id in fetchedModels"
                      :key="id"
                      type="button"
                      class="ah-cli-tag ah-pv-target ah-mono"
                      :class="{ active: modelInList(id) }"
                      :aria-pressed="modelInList(id)"
                      :data-testid="`preset-model-chip-${id}`"
                      @click="toggleFetchedModel(id)"
                    >
                      {{ id }}
                    </button>
                    <button
                      type="button"
                      class="ah-btn ah-btn-ghost ah-btn-sm"
                      data-testid="preset-models-add-all"
                      @click="addAllFetched"
                    >
                      {{ t('hub.pv.editor.modelsAddAll', { n: fetchedModels.length }) }}
                    </button>
                  </div>
                  <div v-else-if="!modelsQueryError && !props.providers.fetchingModels.value && formModels.trim() === ''" class="ah-inst-hint">
                    {{ t('hub.pv.editor.modelsEmpty') }}
                  </div>
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
