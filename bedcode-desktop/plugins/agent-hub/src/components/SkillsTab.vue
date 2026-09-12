<script setup lang="ts">
/**
 * Skills 管理分区（票据 04）：规范库列表 + GitHub 安装 + 本地导入 + 编辑器入口
 *
 * 设计真源：原型 `.scratch/agent-hub/prototype/index.html` #a-sk（页头双动作 +
 * 状态 tag 行 + 技能表格）。规范库 `~/.agents/skills` 为真源，分发到 claude /
 * pi 私有目录（opencode/codex 无 skills 目录约定，仅静态提示）。
 * 动作经 useSkills 单实例（AgentHubView 持有）执行，状态经
 * `plugin:agent-hub:skills` 事件回流；编辑器为独立子组件（SkillEditor）。
 */
import { computed, inject, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, SkillEntry, SkillsDomainState } from '../types'
import type { UseSkillsReturn } from '../composables/useSkills'
import SkillEditor from './SkillEditor.vue'

const props = defineProps<{
  detection: AgentHubState | null
  skills: UseSkillsReturn
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

const TARGET_NAMES = ['claude', 'pi'] as const

/** fs 类动作依赖目录授权；拒绝时置灰降级（与概览/安装页同一横幅语义） */
const authGranted = computed(() => props.detection?.authGranted ?? false)

const state = computed<SkillsDomainState | null>(() => props.skills.state.value)
const scanning = computed(() => state.value?.status === 'scanning')
const importing = computed(() => state.value?.importing === true)
const busy = computed(() => scanning.value || importing.value || props.skills.githubBusy.value)

// ==================== 规范库行视图 ====================

interface SkillRow {
  entry: SkillEntry
  /** 每目标分发状态（仅 distributed/stale 出标签；全 none 时整体「仅规范库」） */
  tags: { target: string; status: 'distributed' | 'stale' }[]
}

const rows = computed<SkillRow[]>(() =>
  (state.value?.skills ?? []).map((entry) => ({
    entry,
    tags: TARGET_NAMES.filter((t) => entry.distribution[t]?.status !== 'none').map((t) => ({
      target: t,
      status: entry.distribution[t].status as 'distributed' | 'stale',
    })),
  })),
)

/** 汇总：某目标全部 distributed → 已同步；有 stale/missing → N 落后 */
function targetSummary(target: 'claude' | 'pi'): { status: 'ok' | 'warn' | 'neutral'; count: number } {
  const skills = state.value?.skills ?? []
  let bad = 0
  let good = 0
  for (const s of skills) {
    const st = s.distribution[target]?.status
    if (st === 'distributed') good++
    else bad++
  }
  if (skills.length === 0) return { status: 'neutral', count: 0 }
  if (bad === 0) return { status: 'ok', count: 0 }
  return { status: 'warn', count: bad }
}

// ==================== GitHub 安装 ====================

const showGithubForm = ref(false)
const githubUrl = ref('')
/** 同名 skill 覆盖确认（install 返回 exists 名单后出现） */
const githubExists = ref<string[] | null>(null)
/** 不可达/失败提示（来自命令返回或状态 github.last.error） */
const githubError = ref<string | null>(null)

async function installGithub(overwrite: boolean) {
  const url = githubUrl.value.trim()
  if (!url) return
  githubExists.value = null
  githubError.value = null
  const result = await props.skills.installGithub(url, overwrite)
  if (result === null) return
  if (result.exists && result.exists.length > 0) {
    githubExists.value = result.exists
    return
  }
  if (result.installed) {
    githubExists.value = null
    githubUrl.value = ''
    showGithubForm.value = false
    return
  }
  githubError.value = result.error ?? t('hub.skill.github.failed')
}

// ==================== 本地导入 ====================

/** 目标同名 skill 覆盖确认（import 返回 exists 后出现） */
const importPending = ref<{ path: string; name: string } | null>(null)
const importAuthDenied = ref(false)

async function importLocal(force?: boolean) {
  importAuthDenied.value = false
  if (!force && importPending.value) {
    // 覆盖确认路径：携已选 path 重入
    const { path } = importPending.value
    const r = await props.skills.importLocal({ path, force: true })
    handleImportResult(r)
    return
  }
  importPending.value = null
  const r = await props.skills.importLocal({})
  handleImportResult(r)
}

function handleImportResult(r: Awaited<ReturnType<UseSkillsReturn['importLocal']>>) {
  if (r === null || !r.picked) return
  if (r.auth === false) {
    importAuthDenied.value = true
    return
  }
  if (r.exists && r.path && r.name) {
    importPending.value = { path: r.path, name: r.name }
    return
  }
  importPending.value = null
}

// ==================== 编辑 ====================

const editingDir = ref<string | null>(null)

function openEditor(dir: string) {
  editingDir.value = dir
}

function closeEditor() {
  editingDir.value = null
}
</script>

<template>
  <div class="ah-sk">
    <div v-if="!authGranted" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
    </div>

    <!-- 编辑器（替换列表视图） -->
    <SkillEditor v-if="editingDir" :skills="skills" :dir="editingDir" @close="closeEditor" />

    <template v-else>
      <div class="ah-inst-head">
        <span class="ah-section-title">{{ t('hub.tab.skills') }}</span>
        <span class="ah-speed-actions-btns">
          <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :disabled="busy" @click="skills.scan()">
            {{ scanning ? t('hub.skill.scanning') : t('hub.skill.scan') }}
          </button>
          <button
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="authGranted === false"
            data-testid="import-local"
            @click="importLocal()"
          >
            {{ t('hub.skill.import.action') }}
          </button>
          <button
            type="button"
            class="ah-btn ah-btn-primary ah-btn-sm"
            :disabled="authGranted === false"
            data-testid="open-github"
            @click="showGithubForm = !showGithubForm"
          >
            {{ t('hub.skill.github.action') }}
          </button>
        </span>
      </div>

      <!-- 状态 tag 行（原型 #a-sk） -->
      <div class="ah-sk-status">
        <span class="ah-cli-tag">
          <span class="ah-cli-dot"></span>{{ t('hub.skill.library', { root: state?.libraryRoot ?? '', n: state?.skills.length ?? 0 }) }}
        </span>
        <span
          v-for="target in TARGET_NAMES"
          :key="target"
          class="ah-cli-tag"
          :class="targetSummary(target).status === 'ok' ? 'ok' : targetSummary(target).status === 'warn' ? 'warn' : ''"
        >
          <span class="ah-cli-dot"></span>
          {{ targetSummary(target).status === 'ok'
            ? t('hub.skill.targetSync', { name: target })
            : targetSummary(target).status === 'warn'
              ? t('hub.skill.targetStale', { name: target, n: targetSummary(target).count })
              : t('hub.skill.targetIdle', { name: target }) }}
        </span>
        <span class="ah-cli-tag">{{ t('hub.skill.targetNoConvention') }}</span>
      </div>

      <!-- 扫描/导入失败提示 -->
      <div v-if="state?.status === 'error' && state?.error" class="ah-cli-error">
        {{ t('hub.skill.scanError', { error: state.error }) }}
      </div>

      <!-- GitHub 安装表单 -->
      <div v-if="showGithubForm" class="ah-card" data-testid="github-form">
        <div class="ah-inst-head">
          <span class="ah-section-title">{{ t('hub.skill.github.title') }}</span>
        </div>
        <div class="ah-sk-github-row">
          <input
            v-model="githubUrl"
            class="ah-sk-url ah-mono"
            type="text"
            :placeholder="t('hub.skill.github.urlPlaceholder')"
            spellcheck="false"
            data-testid="github-url"
          />
          <button
            type="button"
            class="ah-btn ah-btn-primary ah-btn-sm"
            :disabled="busy || !githubUrl.trim()"
            @click="installGithub(false)"
          >
            {{ skills.githubBusy.value ? t('hub.skill.github.installing') : t('hub.skill.github.install') }}
          </button>
        </div>
        <div class="ah-inst-hint ah-sk-github-hint">{{ t('hub.skill.github.hint') }}</div>

        <div v-if="githubExists" class="ah-banner ah-sk-confirm">
          <span class="ah-banner-ic">⚠</span>
          <span class="ah-banner-text">{{ t('hub.skill.github.exists', { names: githubExists.join(', ') }) }}</span>
          <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="installGithub(true)">
            {{ t('hub.skill.github.overwrite') }}
          </button>
        </div>

        <div v-if="githubError" class="ah-cli-error">
          {{ t('hub.skill.github.unreachable') }}
          <div class="ah-sk-raw-error">{{ githubError }}</div>
        </div>
      </div>

      <!-- 导入覆盖确认 / 授权拒绝 -->
      <div v-if="importPending" class="ah-banner ah-sk-confirm" data-testid="import-confirm">
        <span class="ah-banner-ic">⚠</span>
        <span class="ah-banner-text">{{ t('hub.skill.import.exists', { name: importPending.name }) }}</span>
        <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" @click="importLocal(true)">
          {{ t('hub.skill.import.overwrite') }}
        </button>
      </div>
      <div v-if="importAuthDenied" class="ah-cli-error">{{ t('hub.skill.import.authDenied') }}</div>

      <!-- 导入/GitHub 结果（guest 状态回流） -->
      <div v-if="state?.import?.last" class="ah-sk-result" :class="{ err: !state.import.last.ok }">
        {{ state.import.last.ok
          ? t('hub.skill.import.done', { name: state.import.last.name, n: state.import.last.fileCount })
          : t('hub.skill.import.failed', { error: state.import.last.error ?? '' }) }}
      </div>
      <div v-if="state?.github?.last?.ok" class="ah-sk-result">
        {{ t('hub.skill.github.done', { n: state.github.last.installed.length })
          }}<template v-if="state.github.last.skippedFiles > 0"> · {{ t('hub.skill.github.skipped', { n: state.github.last.skippedFiles }) }}</template>
      </div>

      <!-- 空态 -->
      <div v-if="!scanning && rows.length === 0" class="ah-card ah-sk-empty">
        <div class="ah-section-title">{{ t('hub.skill.empty') }}</div>
        <div class="ah-inst-hint ah-sk-empty-hint">{{ t('hub.skill.emptyHint') }}</div>
      </div>

      <!-- 技能表格（原型：技能 | 描述 | 分发状态 | 动作） -->
      <div v-if="rows.length > 0" class="ah-card ah-sk-table-card">
        <div v-for="row in rows" :key="row.entry.dir" class="ah-sk-row" :data-testid="`skill-row-${row.entry.dir}`">
          <div class="ah-sk-row-main">
            <span class="ah-sk-row-name">
              <span class="ah-sk-row-name-text">{{ row.entry.name }}</span>
              <span class="ah-mono ah-sk-row-dir">{{ row.entry.dir }}</span>
            </span>
            <span class="ah-sk-row-desc" :title="row.entry.description ?? ''">
              {{ row.entry.description ?? row.entry.error ?? '' }}
            </span>
          </div>
          <div class="ah-sk-row-dist">
            <span v-if="row.tags.length === 0" class="ah-cli-tag">
              <span class="ah-cli-dot"></span>{{ t('hub.skill.status.libraryOnly') }}
            </span>
            <span
              v-for="tag in row.tags"
              :key="tag.target"
              class="ah-cli-tag"
              :class="tag.status === 'distributed' ? 'ok' : 'warn'"
            >
              <span class="ah-cli-dot"></span>
              {{ tag.status === 'distributed'
                ? t('hub.skill.status.synced', { name: tag.target })
                : t('hub.skill.status.stale', { name: tag.target }) }}
            </span>
          </div>
          <div class="ah-sk-row-actions">
            <button type="button" class="ah-btn ah-btn-ghost ah-btn-sm" :data-testid="`edit-${row.entry.dir}`" @click="openEditor(row.entry.dir)">
              {{ t('hub.skill.edit') }}
            </button>
            <button
              type="button"
              class="ah-btn ah-btn-sm"
              :class="row.tags.some((x) => x.status === 'stale') || row.tags.length === 0 ? 'ah-btn-primary' : 'ah-btn-ghost'"
              :disabled="busy || props.skills.distributing.value !== null || !authGranted"
              :data-testid="`distribute-${row.entry.dir}`"
              @click="skills.distribute(row.entry.dir)"
            >
              {{ props.skills.distributing.value === row.entry.dir
                ? t('hub.skill.distributing')
                : row.tags.length === 0
                  ? t('hub.skill.distribute')
                  : row.tags.some((x) => x.status === 'stale')
                    ? t('hub.skill.redistribute')
                    : t('hub.skill.distribute') }}
            </button>
          </div>
        </div>
      </div>
    </template>
  </div>
</template>
