<script setup lang="ts">
/**
 * 概览分区（票据 02 交付范围 + 票据 03 测速行）：目录授权横幅 + 环境条
 * （含 npm 源测速与推荐）+ 四张 CLI 卡片
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { AgentHubState, CliId, InstallDomainState } from '../types'
import CliCard from './CliCard.vue'

const props = defineProps<{
  state: AgentHubState | null
  detecting: boolean
  installState: InstallDomainState | null
  speedTesting: boolean
}>()
const emit = defineEmits<{ detect: []; auth: []; 'speed-test': []; 'goto-install': [] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string) => context.i18n.t(key)

const CLI_IDS: CliId[] = ['claude', 'codex', 'opencode', 'pi']

const envRows = computed(() => {
  const env = props.state?.env
  return [
    { label: t('hub.env.node'), value: env?.node },
    { label: t('hub.env.npm'), value: env?.npm },
    { label: t('hub.env.pnpm'), value: env?.pnpm },
    { label: t('hub.env.registry'), value: env?.registry },
  ]
})

const speed = computed(() => props.installState?.mirror?.speed ?? null)
const speedDone = computed(() => speed.value?.status === 'ok')
const speedFailed = computed(() => speed.value?.status === 'error')
const recommendMirror = computed(() => speedDone.value && speed.value?.recommend === 'npmmirror')
</script>

<template>
  <div class="ah-overview">
    <div v-if="state && !state.authGranted" class="ah-banner">
      <span class="ah-banner-ic">⚠</span>
      <span class="ah-banner-text">{{ t('hub.auth.banner') }}</span>
      <button type="button" class="ah-btn ah-btn-sm" @click="emit('auth')">
        {{ t('hub.auth.action') }}
      </button>
    </div>

    <div class="ah-card">
      <div class="ah-env-head">
        <span class="ah-section-title">{{ t('hub.env.title') }}</span>
        <button
          type="button"
          class="ah-btn ah-btn-ghost ah-btn-sm"
          :disabled="detecting"
          @click="emit('detect')"
        >
          {{ detecting ? t('hub.env.detecting') : t('hub.env.detect') }}
        </button>
      </div>
      <div class="ah-env-rows">
        <div v-for="row in envRows" :key="row.label" class="ah-env-row">
          <span class="ah-env-label">{{ row.label }}</span>
          <span class="ah-env-value ah-mono">{{ row.value ?? t('hub.env.none') }}</span>
        </div>
      </div>

      <!-- npm 源测速（原型 b-ov：两源计时 + 推荐；动作在安装与更新页） -->
      <div class="ah-speed-line">
        <span v-if="speedDone" class="ah-speed-text">
          {{ t('hub.speed.official') }} <b class="ah-mono">{{ speed?.npmjsMs ?? '—' }} ms</b>
          <span class="ah-speed-vs">vs</span>
          {{ t('hub.speed.mirror') }} <b class="ah-mono">{{ speed?.npmmirrorMs ?? '—' }} ms</b>
          <span class="ah-cli-tag ah-speed-tag" :class="recommendMirror ? 'warn' : 'ok'">
            <span class="ah-cli-dot"></span>{{ recommendMirror ? t('hub.speed.recommendMirror') : t('hub.speed.recommendOfficial') }}
          </span>
        </span>
        <span v-else-if="speedFailed" class="ah-speed-text ah-cli-error">{{ speed?.error ?? t('hub.speed.fail') }}</span>
        <span v-else class="ah-speed-text"></span>
        <span class="ah-speed-line-btns">
          <button
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            :disabled="speedTesting"
            @click="emit('speed-test')"
          >
            {{ speedTesting ? t('hub.speed.testing') : t('hub.speed.action') }}
          </button>
          <button
            v-if="recommendMirror"
            type="button"
            class="ah-btn ah-btn-ghost ah-btn-sm"
            @click="emit('goto-install')"
          >
            {{ t('hub.speed.goto') }}
          </button>
        </span>
      </div>
    </div>

    <div class="ah-grid">
      <CliCard v-for="id in CLI_IDS" :key="id" :cli-id="id" :info="state?.clis?.[id] ?? null" />
    </div>
  </div>
</template>
