<script setup lang="ts">
/**
 * 预设应用面板（票据 05）：选目标 CLI → key 三选一 → 写入目标配置文件
 *
 * key 纪律：inline 现场输入（不持久化）、source 内存直拷（guest 应用时现读
 * 源 CLI 配置）、none 保留目标既有凭据；源信息来自预设 notes（反向导入标注），
 * 源值只以掩码回显（导入结果的 keys）。
 * claude 桥接冲突：guest 检测到 provider-config.sh / anthropic-bridge.mjs 时
 * 拒绝写入返回 bridgeConflict，面板呈现冲突说明，用户确认后携 force 重试——
 * 桥接文件永不触碰，仅写 settings.json 的 env 块。
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #a-pv「应用 sensenova → Pi」卡。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { ProviderPreset } from '../types'
import { APPLY_TARGETS, sourceFromNotes } from '../utils/providers'
import type { UseProvidersReturn } from '../composables/useProviders'

const props = defineProps<{
  providers: UseProvidersReturn
  preset: ProviderPreset
}>()

const emit = defineEmits<{ close: [] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

// ==================== 表单状态 ====================

const TARGET_PATHS: Record<string, string> = {
  claude: '~/.claude/settings.json',
  pi: '~/.pi/agent/{models.json, auth.json}',
  opencode: '~/.config/opencode/opencode.json',
}

const target = ref<string>('pi')
const targetName = ref(props.preset.name)
const keyMode = ref<'inline' | 'source' | 'none'>(
  sourceFromNotes(props.preset.notes) ? 'source' : 'inline',
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
const error = ref<string | null>(null)
const appliedFiles = ref<string[] | null>(null)

async function apply(force: boolean) {
  error.value = null
  conflict.value = null
  const key =
    keyMode.value === 'inline'
      ? { kind: 'inline', value: keyValue.value }
      : keyMode.value === 'source' && source.value
        ? { kind: 'source', cli: source.value.cli, provider: source.value.provider }
        : { kind: 'none' }
  const result = await props.providers.applyProvider(
    props.preset.id,
    target.value,
    targetName.value,
    key,
    force,
  )
  if (result === null) return
  if (result.bridgeConflict) {
    conflict.value = result.bridges ?? []
    return
  }
  if (result.applied) {
    appliedFiles.value = result.files ?? []
    keyValue.value = ''
  } else if (result.error) {
    error.value = result.error
  }
}
</script>

<template>
  <div class="ah-card ah-pv-apply" data-testid="provider-apply">
    <div class="ah-inst-head">
      <span class="ah-section-title">{{ t('hub.pv.apply.title', { name: preset.name }) }}</span>
      <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" data-testid="apply-close" @click="emit('close')">
        ×
      </button>
    </div>

    <!-- 写入成功：文件清单 + 重启提示 -->
    <div v-if="appliedFiles" class="ah-sk-result" data-testid="apply-done">
      {{ t('hub.pv.apply.restartHint', { files: appliedFiles.join(', ') }) }}
    </div>

    <template v-else>
      <!-- 目标 CLI（codex 待校准，置灰提示） -->
      <div class="ah-pv-field">
        <span class="ah-pv-label">{{ t('hub.pv.apply.target') }}</span>
        <span class="ah-pv-targets">
          <button
            v-for="tg in APPLY_TARGETS"
            :key="tg"
            type="button"
            class="ah-cli-tag ah-pv-target"
            :class="{ active: target === tg }"
            :data-testid="`target-${tg}`"
            @click="target = tg"
          >
            {{ tg }}
          </button>
          <span class="ah-cli-tag" :title="t('hub.pv.apply.codexUnsupported')">codex · v1 ✕</span>
        </span>
      </div>
      <div class="ah-inst-hint">{{ TARGET_PATHS[target] }}</div>

      <!-- 配置条目名（写入目标文件中的键名） -->
      <div class="ah-pv-field">
        <span class="ah-pv-label">{{ t('hub.pv.apply.targetName') }}</span>
        <input v-model="targetName" class="ah-sk-url ah-pv-input ah-mono" type="text" spellcheck="false" />
      </div>
      <div class="ah-inst-hint">{{ t('hub.pv.apply.targetNameHint', { target }) }}</div>

      <!-- key 三选一 -->
      <div class="ah-pv-field">
        <span class="ah-pv-label">{{ t('hub.pv.apply.keyMode') }}</span>
        <span class="ah-pv-keymodes">
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
      </div>
      <input
        v-if="keyMode === 'inline'"
        v-model="keyValue"
        class="ah-sk-url ah-pv-input ah-mono"
        type="password"
        autocomplete="off"
        data-testid="apply-key-input"
      />
      <div v-if="keyMode === 'source' && sourceMask" class="ah-inst-hint ah-mono">
        {{ t('hub.pv.apply.keySourceMask', { mask: sourceMask }) }}
      </div>

      <!-- claude 桥接现状提示（state 只读视图） -->
      <div v-if="target === 'claude' && claudeBridged && !conflict" class="ah-banner ah-sk-confirm">
        <span class="ah-banner-ic">⚠</span>
        <span class="ah-banner-text">{{ t('hub.pv.claude.bridge', { files: 'provider-config.sh / anthropic-bridge.mjs' }) }}</span>
      </div>

      <!-- 桥接冲突两击确认（guest 拒绝后出现；桥接文件不覆盖） -->
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

      <div v-if="error" class="ah-cli-error">{{ t('hub.pv.apply.failed', { error }) }}</div>

      <div class="ah-pv-apply-actions">
        <button
          type="button"
          class="ah-btn ah-btn-primary ah-btn-sm"
          :disabled="providers.applying.value || (keyMode === 'inline' && !keyValue)"
          data-testid="apply-write"
          @click="apply(false)"
        >
          {{ providers.applying.value ? t('hub.pv.apply.writing') : t('hub.pv.apply.write') }}
        </button>
      </div>
    </template>
  </div>
</template>
