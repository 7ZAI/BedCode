<script setup lang="ts">
/**
 * Skill 编辑器（票据 04）：SKILL.md 编辑 + 保存前 diff 预览 + 冲突处理
 *
 * 保存流（spec §4.3「mtime/hash 冲突检测 + diff 预览」的 hash 语义）：
 * 1. 装载时记录磁盘基线 baseContent → 编辑产生 draft
 * 2. 「保存」先算 base↔draft 的行级 diff 呈现预览（utils/diff.ts），二次确认才落盘
 * 3. guest 保存前重读磁盘做冲突检测：磁盘内容 ≠ baseContent 时返回磁盘现状，
 *    编辑器转冲突态——呈现 磁盘现状↔draft 的 diff，由用户选择
 *    「以我的版本覆盖」（force）或「从磁盘重读」（放弃草稿）
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html`（表格行「编辑」入口）。
 */
import { computed, ref, watch } from 'vue'
import { inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import MarkdownEditor from '@binblink/bedcode-plugin-sdk-desktop/ui/markdown-editor'
import { diffLines, diffStats } from '../utils/diff'
import type { SkillDetail } from '../types'
import type { UseSkillsReturn } from '../composables/useSkills'

const props = defineProps<{
  skills: UseSkillsReturn
  dir: string | null
}>()

const emit = defineEmits<{ close: [] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

/** 装载的 skill 详情（content 为磁盘基线） */
const detail = ref<SkillDetail | null>(null)
/** 草稿内容（textarea 双向绑定） */
const draft = ref('')
/** 加载中 */
const loading = ref(false)
/** 保存中（等待 guest 命令返回） */
const saving = ref(false)
/** diff 预览态（保存两击确认的第一击） */
const previewing = ref(false)
/** 冲突态：guest 返回的磁盘现状 */
const conflictCurrent = ref<string | null>(null)
/** 保存成功瞬态提示 */
const savedFlash = ref(false)
let savedTimer: ReturnType<typeof setTimeout> | null = null

async function load(dir: string) {
  loading.value = true
  previewing.value = false
  conflictCurrent.value = null
  detail.value = null
  try {
    const d = await props.skills.readSkill(dir)
    detail.value = d
    draft.value = d?.content ?? ''
  } finally {
    loading.value = false
  }
}

watch(
  () => props.dir,
  (dir) => {
    if (dir) void load(dir)
  },
  { immediate: true },
)

const dirty = computed(() => detail.value !== null && draft.value !== detail.value.content)

/** 预览态 diff：基线 ↔ 草稿 */
const previewLines = computed(() =>
  previewing.value && detail.value ? diffLines(detail.value.content, draft.value) : [],
)
const previewStats = computed(() => diffStats(previewLines.value))
const hasPreviewChanges = computed(() => previewStats.value.added + previewStats.value.removed > 0)

/** 冲突态 diff：磁盘现状 ↔ 草稿（覆盖将带来的全部变化） */
const conflictLines = computed(() =>
  conflictCurrent.value !== null ? diffLines(conflictCurrent.value, draft.value) : [],
)
const conflictStats = computed(() => diffStats(conflictLines.value))

function onPreviewSave() {
  if (!detail.value) return
  previewing.value = true
}

function cancelPreview() {
  previewing.value = false
}

async function confirmSave() {
  const d = detail.value
  if (!d) return
  saving.value = true
  try {
    const result = await props.skills.saveSkill(d.dir, d.content, draft.value, false)
    if (result.conflict) {
      // 冲突：磁盘已被外部修改 → 转冲突态（diff 基准切换为磁盘现状）
      previewing.value = false
      conflictCurrent.value = result.current ?? ''
      return
    }
    if (result.saved) {
      // 以保存后的内容为新基线；状态刷新经事件回流（分发徽标随之更新）
      detail.value = { ...d, content: draft.value }
      previewing.value = false
      savedFlash.value = true
      if (savedTimer) clearTimeout(savedTimer)
      savedTimer = setTimeout(() => {
        savedFlash.value = false
      }, 2500)
    }
  } finally {
    saving.value = false
  }
}

async function overwriteMine() {
  const d = detail.value
  const current = conflictCurrent.value
  if (!d) return
  saving.value = true
  try {
    const result = await props.skills.saveSkill(d.dir, current ?? '', draft.value, true)
    if (result.saved) {
      detail.value = { ...d, content: draft.value }
      conflictCurrent.value = null
      savedFlash.value = true
      if (savedTimer) clearTimeout(savedTimer)
      savedTimer = setTimeout(() => {
        savedFlash.value = false
      }, 2500)
    }
  } finally {
    saving.value = false
  }
}

async function reloadFromDisk() {
  if (detail.value) await load(detail.value.dir)
}
</script>

<template>
  <div class="ah-sk-editor">
    <div class="ah-inst-head">
      <span class="ah-section-title">
        {{ t('hub.skill.editor.title') }} ·
        <span class="ah-mono">{{ dir }}</span>
      </span>
      <span class="ah-speed-actions-btns">
        <span v-if="savedFlash" class="ah-cli-tag ok"><span class="ah-cli-dot"></span>{{ t('hub.skill.editor.saved') }}</span>
        <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="saving" @click="emit('close')">
          {{ t('hub.skill.editor.back') }}
        </button>
      </span>
    </div>

    <div v-if="loading" class="ah-sk-loading">{{ t('hub.card.detecting') }}</div>
    <template v-else-if="detail">
      <div v-if="detail.error" class="ah-cli-error">{{ detail.error }}</div>
      <div class="ah-sk-meta">
        <span class="ah-env-row"><span class="ah-env-label">name</span><span class="ah-env-value ah-mono">{{ detail.name }}</span></span>
        <span v-if="detail.allowedTools" class="ah-env-row"><span class="ah-env-label">allowed-tools</span><span class="ah-env-value ah-mono">{{ detail.allowedTools }}</span></span>
      </div>

      <!-- 冲突态 -->
      <template v-if="conflictCurrent !== null">
        <div class="ah-banner">
          <span class="ah-banner-ic">⚠</span>
          <span class="ah-banner-text">{{ t('hub.skill.editor.conflict') }}</span>
        </div>
        <div class="ah-sk-diff" data-testid="conflict-diff">
          <div
            v-for="(line, idx) in conflictLines"
            :key="idx"
            class="ah-sk-diff-line"
            :class="line.type"
          >{{ line.type === 'add' ? '+' : line.type === 'del' ? '-' : ' ' }} {{ line.text }}</div>
        </div>
        <div class="ah-sk-diff-actions">
          <span class="ah-inst-hint">+{{ conflictStats.added }} / −{{ conflictStats.removed }}</span>
          <span class="ah-speed-actions-btns">
            <button type="button" class="ah-btn ah-btn-warn ah-btn-ghost ah-btn-sm" :disabled="saving" @click="overwriteMine">
              {{ t('hub.skill.editor.overwriteMine') }}
            </button>
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="saving" @click="reloadFromDisk">
              {{ t('hub.skill.editor.reload') }}
            </button>
          </span>
        </div>
      </template>

      <!-- 编辑态 / 保存预览态 -->
      <template v-else>
        <!-- 编辑区：占满面板剩余高度（ah-sk-editor-body flex:1），底部内容不贴边（组件内 padding） -->
        <div class="ah-sk-editor-body">
          <MarkdownEditor v-model="draft" :disabled="previewing || saving" />
        </div>

        <!-- 保存前 diff 预览（两击确认第一击后出现） -->
        <template v-if="previewing">
          <div class="ah-section-title ah-sk-preview-title">{{ t('hub.skill.editor.previewTitle') }}</div>
          <div class="ah-sk-diff" data-testid="preview-diff">
            <div
              v-for="(line, idx) in previewLines"
              :key="idx"
              class="ah-sk-diff-line"
              :class="line.type"
            >{{ line.type === 'add' ? '+' : line.type === 'del' ? '-' : ' ' }} {{ line.text }}</div>
          </div>
          <div class="ah-sk-diff-actions">
            <span class="ah-inst-hint">+{{ previewStats.added }} / −{{ previewStats.removed }}</span>
            <span class="ah-speed-actions-btns">
              <button
                type="button"
                class="ah-btn ah-btn-primary ah-btn-sm"
                :disabled="saving || !hasPreviewChanges"
                @click="confirmSave"
              >
                {{ t('hub.skill.editor.confirmSave') }}
              </button>
              <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="saving" @click="cancelPreview">
                {{ t('hub.skill.editor.continueEdit') }}
              </button>
            </span>
          </div>
        </template>

        <div v-else class="ah-sk-diff-actions">
          <span v-if="!dirty" class="ah-inst-hint">{{ t('hub.skill.editor.clean') }}</span>
          <button
            type="button"
            class="ah-btn ah-btn-primary ah-btn-sm"
            :disabled="!dirty || saving"
            data-testid="skill-save"
            @click="onPreviewSave"
          >
            {{ t('hub.skill.editor.save') }}
          </button>
        </div>
      </template>
    </template>
  </div>
</template>
