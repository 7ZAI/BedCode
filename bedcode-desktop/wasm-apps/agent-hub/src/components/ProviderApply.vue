<script setup lang="ts">
/**
 * 预设应用面板（票据 05 / v2 中心凭据库）：选目标 CLI（可多选）→ key 四选一 →
 * 写入各目标配置文件
 *
 * key 纪律：stored 用中心库已存 key（guest 现读库内明文，前端不接触）、
 * inline 现场输入（不持久化）、source 内存直拷（guest 应用时现读源 CLI 配置）、
 * none 保留目标既有凭据；源信息来自预设 notes（反向导入标注），源值只以
 * 掩码回显（导入结果的 keys）。预设已有 stored key 时默认选中。
 * claude 桥接冲突：guest 检测到 provider-config.sh / anthropic-bridge.mjs 时
 * 拒绝该目标并返回 bridgeConflict，面板呈现冲突说明，用户确认后携 force 重试
 * ——桥接文件永不触碰，仅写 settings.json 的 env 块。
 *
 * 多目标：一次可写多个 CLI（顺序即写入顺序）。guest 逐目标给结果，**一个
 * 目标失败不牵连其余目标**——面板逐行呈现结局，不做全有或全无。
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #a-pv「应用 sensenova → Pi」卡。
 */
import { computed, inject, ref } from 'vue'
import { toast } from 'vue-sonner'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type {
  ApplyKeySpec,
  ApplyTargetReason,
  ApplyTargetResult,
  ProviderPreset,
  ProviderTarget,
} from '../types'
import { APPLY_TARGETS, deriveEnvKeyName, sourceFromNotes, TARGET_PATHS } from '../utils/providers'
import type { UseProvidersReturn } from '../composables/useProviders'

const props = defineProps<{
  providers: UseProvidersReturn
  preset: ProviderPreset
}>()

const emit = defineEmits<{ close: [] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

// ==================== 表单状态 ====================

/** 已选目标（默认 pi：唯一支持模型列表的目标 CLI） */
const targets = ref<ProviderTarget[]>(['pi'])
const targetName = ref(props.preset.name)

/** codex 的 env_key 变量名（默认由预设名派生，用户可改成供应商文档里的名字） */
const codexEnvKey = ref(deriveEnvKeyName(props.preset.name))

/** codex 是否在本次目标里（决定 codex 专属表单与「顶掉当前模型」提示） */
const codexSelected = computed(() => targets.value.includes('codex'))

/** codex 当前模型/供应商（只读视图；读失败未授权时为 null，面板不显示该行） */
const codexView = computed(() => props.providers.state.value?.codex ?? null)
const codexCurrent = computed(() => {
  const v = codexView.value
  if (!v || v.denied || !v.model) return null
  return { model: v.model, provider: v.modelProvider }
})

/** 应用到 codex = 把当前模型切到预设首个模型（codex 无模型清单，只能指一个） */
const codexNextModel = computed(() => props.preset.models[0] ?? null)
/** 会顶掉当前模型时才提示（切到同一个不算破坏） */
const codexSwitch = computed(() => {
  const cur = codexCurrent.value
  const next = codexNextModel.value
  if (!cur || !next) return null
  if (cur.model === next) return null
  return { from: cur, to: { model: next, provider: targetName.value } }
})

/** codex 只讲 Responses：非 openai/custom 方言无对应写法（guest 也会拒） */
const codexDialectBlocked = computed(
  () => codexSelected.value && !['openai', 'custom'].includes(props.preset.apiStyle),
)

function toggleTarget(tg: ProviderTarget) {
  const i = targets.value.indexOf(tg)
  if (i >= 0) targets.value = targets.value.filter((x) => x !== tg)
  else targets.value = [...targets.value, tg]
}

/** claude 的 targetName 不参与写入（只改 env 块），单独说明避免误解 */
const nameHintTarget = computed(() => targets.value.join(' · '))

/** stored = 中心库已存 key（预设 keyMask 非 "—" 时默认）；否则退回 source/inline */
const storedKey = computed(
  () => !!props.preset.keyMask && props.preset.keyMask !== '—',
)
const keyMode = ref<'inline' | 'stored' | 'source' | 'none'>(
  storedKey.value
    ? 'stored'
    : sourceFromNotes(props.preset.notes)
      ? 'source'
      : 'inline',
)
const keyValue = ref('')

/** key 直拷源（来自反向导入标注）；源值掩码来自导入结果 */
const source = computed(() => sourceFromNotes(props.preset.notes))
const sourceMask = computed(
  () => props.providers.state.value?.import.last?.keys[props.preset.name] ?? null,
)

/** claude 桥接现状（state 只读视图，随事件回流刷新） */
const claudeBridged = computed(() => {
  const b = props.providers.state.value?.claude.bridge
  return !!(b?.providerConfigSh || b?.anthropicBridgeMjs)
})

/** 桥接冲突确认态（guest 返回 bridgeConflict 后出现） */
const conflict = ref<string[] | null>(null)
/** 错误提示（i18n key + 插值参数；null = 无错误） */
const error = ref<{ key: string; params?: Record<string, unknown> } | null>(null)
/** 逐目标结局（成功=写入文件清单，失败=分类 reason + 原文只进 console） */
const applied = ref<ApplyTargetResult[] | null>(null)
/** 部分成功：失败目标收敛后的就地重试态（2026-10-04 OCR A-04）——
 * applied 里存在 ok=false 的行时表单必须保留（可重试失败目标），
 * 全部成功才收起 */
const hasFailedTargets = computed(() => applied.value?.some((r) => !r.ok) ?? false)

/** 纯 codex 应用（codex 不吃 key 值，隐藏 key 四选一并免掉它的必填校验） */
const codexOnly = computed(
  () => targets.value.length === 1 && targets.value[0] === 'codex',
)

/** 动作条左侧回显：本次将写入哪些目标（空选时点名，避免只看到一个灰按钮） */
const applyNote = computed(() =>
  targets.value.length === 0
    ? t('hub.pv.apply.noteNone')
    : t('hub.pv.apply.note', { n: targets.value.length, targets: targets.value.join(' · ') }),
)

/** 预设没有模型：pi / opencode / codex 写入会被 guest 拒绝（fail-visible），提前拦一道
 * （2026-10-04 OCR A-06：codex 入列——guest 侧 `codex.rs::plan_apply` 同样拒
 * `noModels`，且 `codexNextModel` 为 null 时切换预览横幅也不出现，此前 codex
 * 是唯一在查不到模型的预设上无任何点击前警告的目标） */
const modelLessTargets = computed(() =>
  props.preset.models.length === 0 ? ['pi', 'opencode', 'codex'] : [],
)
const blockedTargets = computed(() =>
  targets.value.filter((tg) => modelLessTargets.value.includes(tg)),
)

/** 失败分类 → i18n（guest 的 error 原文只进 console，不进界面） */
function reasonText(reason: ApplyTargetReason | null): string {
  switch (reason) {
    case 'noModels':
      return t('hub.pv.apply.noModels')
    case 'bridgeConflict':
      return t('hub.pv.apply.reason.bridge')
    case 'writeFailed':
      return t('hub.pv.apply.reason.writeFailed')
    case 'unsupportedDialect':
      return t('hub.pv.apply.reason.dialect')
    case 'invalidEnvKey':
      return t('hub.pv.apply.reason.envKey')
    default:
      return t('hub.pv.apply.failed')
  }
}

async function apply(force: boolean) {
  // 并发守卫：写入按钮 disabled 之外，回车/连点仍可重入；busy 不是失败不提示
  if (props.providers.applying.value) return
  if (targets.value.length === 0) return
  error.value = null
  conflict.value = null
  const key: ApplyKeySpec =
    keyMode.value === 'inline'
      ? { kind: 'inline', value: keyValue.value }
      : keyMode.value === 'stored'
        ? { kind: 'stored' }
        : keyMode.value === 'source' && source.value
          ? { kind: 'source', cli: source.value.cli, provider: source.value.provider }
          : { kind: 'none' }
  const res = await props.providers.applyProvider(
    props.preset.id,
    targets.value,
    targetName.value,
    key,
    force,
    // 只在真选了 codex 时带 env 变量名（否则载荷里出现与本次无关的 codex 字段）
    codexSelected.value ? codexEnvKey.value : '',
  )
  if (res.status === 'busy') return
  if (res.status === 'error') {
    console.error(`[Agent Hub] apply provider command failed (${targets.value.join(',')})`, res.error)
    error.value = { key: 'hub.pv.apply.failed' }
    toast.error(t('hub.pv.apply.failed'))
    return
  }
  const result = res.data
  if (!result) {
    error.value = { key: 'hub.pv.apply.failed' }
    toast.error(t('hub.pv.apply.failed'))
    return
  }
  // 先看逐目标结局：部分成功 + 部分被拒（如 pi 写成功、claude 撞桥接）时，
  // 成功的那几行必须留在面板上，不能被「冲突」提示盖掉
  const results = result.results ?? []
  if (results.length > 0) applied.value = results
  if (result.bridgeConflict) {
    conflict.value = result.bridges ?? []
    return
  }
  // guest 回执异常（既无 results 也无 applied）不装作无事发生
  if (results.length === 0 && !result.applied) {
    error.value = { key: 'hub.pv.apply.failed' }
    toast.error(t('hub.pv.apply.failed'))
    return
  }
  for (const r of results) {
    if (!r.ok) console.error(`[Agent Hub] apply provider failed (${r.target})`, r.reason, r.error)
  }
  keyValue.value = ''
  if (result.applied) {
    // 面板保留：写入文件清单 + 重启提示需可读，不自动关
    toast.success(t('hub.pv.toast.applied', { name: props.preset.name }))
  } else {
    const failed = results.filter((r) => !r.ok)
    if (failed.length > 0 && failed.length < results.length) {
      // 部分成功（2026-10-04 OCR A-04）：成功目标已写入，不弹通用「应用失败」
      // toast；把选中收敛到失败目标，表单保留供就地重试——此前 applied 置真把
      // 表单隐藏，只能关面板重开，而成功目标其实已经写进去了。
      targets.value = failed.map((r) => r.target)
      error.value = { key: 'hub.pv.apply.partial', params: { n: failed.length } }
    } else {
      error.value = { key: 'hub.pv.apply.failed' }
      toast.error(t('hub.pv.apply.failed'))
    }
  }
}
</script>

<template>
  <!--
    「应用到」二级页：整个替换供应商列表视图（ProvidersTab 的 v-if 分支），
    因此本页自带三条导航/结构约定：
    ① 唯一的出口是页头的「返回供应商列表」——早前只有一个角上的 `×`，
       读起来像「关掉弹窗」而不是「回到列表」，用户不知道自己身处哪一层；
    ② 页头带一行「预设速览」（方言 / Base URL / 模型数 / key 掩码）——这些
       决策依据在列表行里有、进入本页后全丢了，用户只能靠记忆判断在写什么；
    ③ 表单体与动作条分离（`.ah-pv-apply-body` / `.ah-pv-apply-actions`），
       字段提示紧贴各自控件（早前提示散在字段外，读成一片文字墙）。
  -->
  <div class="ah-card ah-pv-apply" data-testid="provider-apply">
    <div class="ah-pv-apply-head">
      <button
        type="button"
        class="ah-pv-back"
        data-testid="apply-close"
        @click="emit('close')"
      >
        <svg class="ah-pv-back-ic" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M15.75 19.5L8.25 12l7.5-7.5"
          />
        </svg>
        {{ t('hub.pv.apply.back') }}
      </button>
      <h3 class="ah-section-title">{{ t('hub.pv.apply.title', { name: preset.name }) }}</h3>
      <div class="ah-pv-apply-meta">
        <span class="ah-cli-tag">{{ t(`hub.pv.style.${preset.apiStyle}`) }}</span>
        <span class="ah-pv-apply-meta-item ah-mono">{{ preset.baseUrl || '—' }}</span>
        <span class="ah-pv-apply-meta-item">{{ t('hub.pv.models', { n: preset.models.length }) }}</span>
        <span class="ah-cli-tag ah-mono">{{ t('hub.pv.keyMask', { mask: preset.keyMask }) }}</span>
      </div>
    </div>

    <!-- 桥接冲突：位于两段之外——多目标时其他目标可能已写入成功，
         面板要同时给出「冲突提示 + 已完成的那几行 + 重试入口」 -->
    <div v-if="conflict" class="ah-banner ah-sk-confirm" data-testid="apply-conflict">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.pv.apply.conflict', { files: conflict.join(', ') }) }}</span>
      <button
        type="button"
        class="ah-btn ah-btn-warn ah-btn-ghost ah-btn-sm"
        data-testid="apply-conflict-confirm"
        @click="apply(true)"
      >
        {{ t('hub.pv.apply.conflictConfirm') }}
      </button>
    </div>

    <!-- 写入结局：逐目标一行（多目标可能部分成功） -->
    <div v-if="applied" class="ah-pv-results" data-testid="apply-done">
      <div v-for="r in applied" :key="r.target" class="ah-pv-result-row">
        <span class="ah-cli-tag" :class="r.ok ? 'ok' : 'err'">{{ r.target }}</span>
        <span class="ah-inst-hint ah-pv-result-text">
          <template v-if="r.ok">{{ r.files.join(', ') }}</template>
          <template v-else>{{ reasonText(r.reason) }}</template>
        </span>
      </div>
      <div class="ah-sk-result">{{ t('hub.pv.apply.restartHint', { files: applied.filter((r) => r.ok).flatMap((r) => r.files).join(', ') }) }}</div>
    </div>

    <!-- 表单：全部成功写入后收起；有冲突未确认 / 有失败目标待重试时保持可重试 -->
    <template v-if="!applied || conflict || hasFailedTargets">
      <div class="ah-pv-apply-body">
        <!-- 目标 CLI（多选） -->
        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.apply.target') }}</span>
          <span class="ah-pv-targets">
            <button
              v-for="tg in APPLY_TARGETS"
              :key="tg"
              type="button"
              class="ah-cli-tag ah-pv-target"
              :class="{ active: targets.includes(tg) }"
              :aria-pressed="targets.includes(tg)"
              :data-testid="`target-${tg}`"
              @click="toggleTarget(tg)"
            >
              {{ tg }}
            </button>
          </span>
          <span class="ah-inst-hint">{{ t('hub.pv.apply.targetHint') }}</span>
          <span class="ah-inst-hint ah-mono">{{ targets.map((tg) => TARGET_PATHS[tg]).join('  ·  ') }}</span>
        </div>

        <!-- 无模型的预设：提前拦一道（guest 也会拒，这里让原因先到） -->
        <div v-if="blockedTargets.length > 0" class="ah-banner ah-sk-confirm" data-testid="apply-no-models">
          <span class="ah-banner-ic">⚠</span>
          <span class="ah-banner-text">{{ t('hub.pv.apply.noModelsHint', { targets: blockedTargets.join(' · ') }) }}</span>
        </div>

        <!-- 配置条目名（写入目标文件中的键名；claude 不参与） -->
        <div class="ah-pv-field">
          <span class="ah-pv-label">{{ t('hub.pv.apply.targetName') }}</span>
          <input v-model="targetName" class="ah-input ah-pv-input ah-mono" type="text" spellcheck="false" />
          <span class="ah-inst-hint">{{ t('hub.pv.apply.targetNameHint', { target: nameHintTarget }) }}</span>
        </div>

        <!-- codex 专属：env_key 变量名 + 当前模型切换预告 -->
        <template v-if="codexSelected">
          <div class="ah-pv-field">
            <span class="ah-pv-label">{{ t('hub.pv.apply.codex.envKey') }}</span>
            <input
              v-model="codexEnvKey"
              class="ah-input ah-pv-input ah-mono"
              type="text"
              spellcheck="false"
              data-testid="apply-codex-env-key"
            />
            <span class="ah-inst-hint">{{ t('hub.pv.apply.codex.envKeyHint') }}</span>
          </div>

          <!-- 破坏性预告：会顶掉 codex 当前的 model / model_provider -->
          <div v-if="codexSwitch" class="ah-banner ah-sk-confirm" data-testid="apply-codex-switch">
            <span class="ah-banner-ic">⚠</span>
            <span class="ah-banner-text">
              {{ t('hub.pv.apply.codex.switchHint', { from: `${codexSwitch.from.model}（${codexSwitch.from.provider ?? '—'}）`, to: `${codexSwitch.to.model}（${codexSwitch.to.provider}）` }) }}
            </span>
          </div>
          <div v-if="codexDialectBlocked" class="ah-banner ah-sk-confirm" data-testid="apply-codex-dialect">
            <span class="ah-banner-ic">⚠</span>
            <span class="ah-banner-text">{{ t('hub.pv.apply.codex.dialectBlocked') }}</span>
          </div>
          <span class="ah-inst-hint ah-mono">{{ TARGET_PATHS.codex }}</span>
        </template>

        <!-- key 四选一（纯 codex 应用不需要：codex 只写 env_key 变量名） -->
        <template v-if="!codexOnly">
          <div class="ah-pv-field">
            <span class="ah-pv-label">{{ t('hub.pv.apply.keyMode') }}</span>
            <span class="ah-pv-keymodes">
              <label v-if="storedKey" class="ah-pv-radio ah-pv-keymode">
                <input v-model="keyMode" type="radio" value="stored" />
                {{ t('hub.pv.apply.keyStored', { mask: props.preset.keyMask }) }}
              </label>
              <label class="ah-pv-radio ah-pv-keymode">
                <input v-model="keyMode" type="radio" value="inline" />
                {{ t('hub.pv.apply.keyInline') }}
              </label>
              <label v-if="source" class="ah-pv-radio ah-pv-keymode">
                <input v-model="keyMode" type="radio" value="source" />
                {{ t('hub.pv.apply.keySource', { cli: source.cli, provider: source.provider }) }}
              </label>
              <label class="ah-pv-radio ah-pv-keymode">
                <input v-model="keyMode" type="radio" value="none" />
                {{ t('hub.pv.apply.keyNone') }}
              </label>
            </span>
            <input
              v-if="keyMode === 'inline'"
              v-model="keyValue"
              class="ah-input ah-pv-input ah-mono"
              type="password"
              autocomplete="off"
              data-testid="apply-key-input"
            />
            <span v-if="keyMode === 'stored'" class="ah-inst-hint ah-mono">
              {{ t('hub.pv.apply.keyStoredHint', { mask: props.preset.keyMask }) }}
            </span>
            <span v-if="keyMode === 'source' && sourceMask" class="ah-inst-hint ah-mono">
              {{ t('hub.pv.apply.keySourceMask', { mask: sourceMask }) }}
            </span>
          </div>
        </template>

        <!-- claude 桥接现状提示（state 只读视图） -->
        <div v-if="targets.includes('claude') && claudeBridged && !conflict" class="ah-banner ah-sk-confirm">
          <span class="ah-banner-ic">⚠</span>
          <span class="ah-banner-text">{{ t('hub.pv.claude.bridge', { files: 'provider-config.sh / anthropic-bridge.mjs' }) }}</span>
        </div>
      </div>

      <div v-if="error" class="ah-cli-error" data-testid="apply-error">
        {{ t(error.key, error.params) }}
      </div>

      <!-- 动作条：与表单体分离（上边线 + 右侧主按钮），左侧回显「将要写什么」——
           早前主按钮孤零零贴在表单末尾左侧，读不出这是本页的收口动作 -->
      <div class="ah-pv-apply-actions">
        <span class="ah-inst-hint ah-pv-apply-note" data-testid="apply-note">{{ applyNote }}</span>
        <button
          type="button"
          class="ah-btn ah-btn-primary ah-btn-sm"
          :disabled="providers.applying.value || targets.length === 0 || (keyMode === 'inline' && !keyValue && !codexOnly)"
          data-testid="apply-write"
          @click="apply(false)"
        >
          {{ providers.applying.value ? t('hub.pv.apply.writing') : t('hub.pv.apply.write') }}
        </button>
      </div>
    </template>
  </div>
</template>