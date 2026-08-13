<script setup lang="ts">
/**
 * SettingsSection — 文件传输设置区 (Mobile)
 *
 * 共享目录（Shared Directory）：条目以 SAF URI（content://tree/...）存储，
 * 经系统目录树选择器添加（fileService.pickSharedDirectory，持久化授权重启
 * 仍有效）；免授权特殊条目「app 私有下载目录」由 WASM 派生注入（kind=
 * private_downloads，不可移除）。授权被回收/目录被删 → 条目标记失效，
 * 展示「重新授权」入口（story #10）。旧真实路径条目与手动输入已废除
 * （M1 起上传源严格限于共享目录）。
 * 下载目录只读展示（未显式配置时经 get-settings 解析宿主默认下载目录 AppDownloadsDir）。
 * 并发数 1–8 步进；底部常驻明文传输安全告知（spec §10 transfer.settings.plainWarning）。
 *
 * 同时注册为宿主 SettingsSection（registerSettingsSection），并作为插件内设置页复用。
 *
 * 样式完全复用宿主 settings-group / settings-row / settings-section-title /
 * settings-label / settings-desc 设计语言，字号统一 clamp() 流式缩放；
 * 提示与安全告知统一使用黄色提醒框（ft-warning-box）。
 */
import { ref, inject } from 'vue'
import type { PluginContext } from '@bedcode/plugin-sdk-mobile'
import type { useSettings } from '../composables/useSettings'
import type { SharedRoot } from '../types'
import { KIND_PRIVATE_DOWNLOADS } from '../types'
import { CONCURRENCY_MAX } from '../composables/useSettings'
import type { ReceivingPolicy } from '../types'

type SettingsApi = ReturnType<typeof useSettings>

const props = defineProps<{
  settingsApi: SettingsApi
  t: (key: string, params?: Record<string, any>) => string
}>()

/** 宿主经 PluginViewHost provide 的插件上下文（选择器与 Toast 用） */
const context = inject<PluginContext>('pluginContext')

const t = props.t

/** 系统目录树选择器添加共享目录中 */
const picking = ref(false)

/** 系统目录树选择器选共享目录（取消/失败 toast 提示） */
async function handlePickRoot(): Promise<void> {
  if (!context || picking.value) return
  picking.value = true
  try {
    const result = await props.settingsApi.addRoot()
    if (result === 'duplicate') {
      context.dialogs.showToast(t('transfer.settings.rootDuplicate'), 'warning')
    } else if (result === 'failed') {
      context.dialogs.showToast(t('transfer.settings.pickFailed'), 'error')
    } else if (result === 'unsupported') {
      context.dialogs.showToast(t('transfer.settings.pickUnsupported'), 'error')
    }
    // ok / cancelled（用户取消）静默（目录已入列 / 无需打扰）
  } catch {
    context.dialogs.showToast(t('transfer.settings.pickFailed'), 'error')
  } finally {
    picking.value = false
  }
}

/** 重新授权失效条目（重新选择目录树替换） */
async function handleReauthorize(root: SharedRoot): Promise<void> {
  if (!context || picking.value) return
  picking.value = true
  try {
    const ok = await props.settingsApi.reauthorizeRoot(root)
    if (ok) {
      context.dialogs.showToast(t('transfer.settings.reauthorized'), 'success')
    }
  } catch {
    context.dialogs.showToast(t('transfer.settings.pickFailed'), 'error')
  } finally {
    picking.value = false
  }
}

/** 移除共享目录（免授权特殊条目不可移除） */
async function handleRemoveRoot(id: string): Promise<void> {
  await props.settingsApi.removeRoot(id)
}

function decConcurrency(): void {
  const cur = props.settingsApi.settings.value.concurrency
  if (cur > 1) void props.settingsApi.setConcurrency(cur - 1)
}

function incConcurrency(): void {
  const cur = props.settingsApi.settings.value.concurrency
  if (cur < CONCURRENCY_MAX) void props.settingsApi.setConcurrency(cur + 1)
}

// ==================== v2 接收策略 ====================

/** 接收策略选项（自绘 segmented，禁原生 select） */
const POLICY_OPTIONS: { value: ReceivingPolicy; labelKey: string }[] = [
  { value: 'ask', labelKey: 'transfer.settings.receivingPolicyAsk' },
  { value: 'accept', labelKey: 'transfer.settings.receivingPolicyAccept' },
  { value: 'reject', labelKey: 'transfer.settings.receivingPolicyReject' },
]

/** 切换接收策略（本地生效，发送方不感知） */
async function handleSetPolicy(policy: ReceivingPolicy): Promise<void> {
  const ok = await props.settingsApi.setReceivingPolicy(policy)
  if (ok && context) {
    context.dialogs.showToast(t('transfer.settings.saved'), 'success')
  }
}

/** 同意超时输入（秒，10–600；仅 ask 策略显示）。原生数字输入外观完全自绘
 *（输入框 + 步进按钮），不呈现系统控件外观 */
const timeoutInput = ref('')

/** 输入框聚焦/失焦时与设置值同步 */
function syncTimeoutInput(): void {
  timeoutInput.value = String(props.settingsApi.settings.value.approvalTimeoutSec ?? 60)
}

/** 提交超时（失焦/回车时校验 10–600，越界回弹显示值） */
async function commitTimeout(): Promise<void> {
  const n = Number(timeoutInput.value)
  if (Number.isFinite(n)) {
    await props.settingsApi.setApprovalTimeout(n)
  }
  syncTimeoutInput()
}

/** 超时步进（±10s，clamp 10–600） */
async function stepTimeout(delta: number): Promise<void> {
  const cur = props.settingsApi.settings.value.approvalTimeoutSec ?? 60
  const next = Math.min(Math.max(cur + delta, 10), 600)
  await props.settingsApi.setApprovalTimeout(next)
  syncTimeoutInput()
}
</script>

<template>
  <div class="ft-settings px-4 py-4 pb-8 space-y-6">
    <!-- ==================== 共享目录 ==================== -->
    <section class="space-y-3">
      <h2 class="settings-section-title">{{ t('transfer.settings.sharedRoots') }}</h2>

      <!-- 使用说明：黄色提醒框（与底部明文安全告知同款视觉） -->
      <div class="ft-warning-box">
        <p class="ft-warning-text">{{ t('transfer.settings.addRootHint') }}</p>
      </div>

      <!-- 系统目录树选择器：通栏主按钮（图标 + 文案，44px+ 触控目标） -->
      <button
        class="ft-touch-btn w-full gap-2 rounded-xl text-[var(--mobile-text-on-accent)] bg-[var(--mobile-accent)] active:opacity-80 transition-opacity disabled:opacity-50 ft-settings-btn"
        :disabled="picking"
        @click="handlePickRoot()"
      >
        <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        </svg>
        {{ picking ? t('transfer.settings.picking') : t('transfer.settings.pickRoot') }}
      </button>

      <!-- 目录列表 -->
      <div v-if="(settingsApi?.settings.value.roots.length ?? 0) === 0" class="settings-desc py-1">
        {{ t('transfer.settings.noRoots') }}
      </div>
      <div v-else class="settings-group">
        <div
          v-for="(root, idx) in settingsApi?.settings.value.roots ?? []"
          :key="root.id"
          class="settings-row"
          :class="{ 'ft-row-invalid': !root.authorized }"
        >
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-1.5 min-w-0">
                <span class="settings-label truncate" :title="root.name">{{ root.name }}</span>
                <span v-if="root.kind === KIND_PRIVATE_DOWNLOADS" class="ft-free-badge flex-shrink-0">
                  {{ t('transfer.settings.freeBadge') }}
                </span>
                <span v-else-if="!root.authorized" class="ft-invalid-badge flex-shrink-0">
                  {{ t('transfer.settings.rootInvalid') }}
                </span>
              </div>
              <!-- SAF 条目展示 URI（特殊条目展示真实路径） -->
              <p class="ft-root-uri truncate" :title="root.id">{{ root.id }}</p>
            </div>
          </div>
          <div class="flex items-center gap-1.5 flex-shrink-0">
            <button
              v-if="root.kind !== KIND_PRIVATE_DOWNLOADS && !root.authorized"
              class="ft-reauth-btn"
              :disabled="picking"
              @click="handleReauthorize(root)"
            >
              {{ t('transfer.settings.reauthorize') }}
            </button>
            <button
              v-if="root.kind !== KIND_PRIVATE_DOWNLOADS"
              class="flex-shrink-0 ft-settings-remove-btn"
              @click="handleRemoveRoot(root.id)"
            >
              {{ t('transfer.settings.removeRoot') }}
            </button>
          </div>
        </div>
      </div>
      <p class="settings-desc ft-settings-hint">{{ t('transfer.settings.specialEntryHint') }}</p>
    </section>

    <!-- ==================== 下载目录（只读，未配置时展示默认落盘地址） ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.downloadDir') }}</h2>
      <div class="settings-group">
        <div class="settings-row">
          <div class="flex items-center gap-2 flex-1 min-w-0">
            <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
            <span class="settings-value flex-1 min-w-0 truncate" :class="{ 'ft-settings-unset': !(settingsApi?.settings.value.downloadDir) }">
              {{ settingsApi?.settings.value.downloadDir || t('transfer.settings.noDownloadDir') }}
            </span>
          </div>
        </div>
      </div>
      <p class="settings-desc ft-settings-hint">{{ t('transfer.settings.downloadDirHint') }}</p>
    </section>

    <!-- ==================== 并发数 ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.concurrency') }}</h2>
      <div class="settings-group">
        <div class="settings-row">
          <div class="min-w-0">
            <div class="settings-label">{{ t('transfer.settings.concurrency') }}</div>
            <div class="settings-desc">{{ t('transfer.settings.concurrencyHint') }}</div>
          </div>
          <div class="flex items-center gap-2 flex-shrink-0">
            <button
              class="ft-step-btn"
              @click="decConcurrency()"
            >
              −
            </button>
            <span class="ft-step-value">
              {{ settingsApi?.settings.value.concurrency ?? 3 }}
            </span>
            <button
              class="ft-step-btn"
              @click="incConcurrency()"
            >
              +
            </button>
          </div>
        </div>
      </div>
    </section>

    <!-- ==================== 接收策略（v2） ==================== -->
    <section class="space-y-2">
      <h2 class="settings-section-title">{{ t('transfer.settings.receivingPolicy') }}</h2>
      <div class="settings-group">
        <div class="settings-row ft-policy-row">
          <div class="min-w-0 flex-1">
            <div class="settings-label">{{ t('transfer.settings.receivingPolicy') }}</div>
            <div class="settings-desc">{{ t('transfer.settings.receivingPolicyHint') }}</div>
          </div>
          <!-- 自绘分段控件（禁原生 select）：ask/accept/reject 三档 -->
          <div class="ft-segmented flex-shrink-0" role="radiogroup">
            <button
              v-for="opt in POLICY_OPTIONS"
              :key="opt.value"
              role="radio"
              :aria-checked="(settingsApi?.settings.value.receivingPolicy ?? 'ask') === opt.value"
              class="ft-segmented-item"
              :class="{ 'ft-segmented-item--active': (settingsApi?.settings.value.receivingPolicy ?? 'ask') === opt.value }"
              @click="handleSetPolicy(opt.value)"
            >
              {{ t(opt.labelKey) }}
            </button>
          </div>
        </div>

        <!-- 同意超时（仅 ask 策略显示）：自绘数字输入（禁原生 input 外观） -->
        <div v-if="(settingsApi?.settings.value.receivingPolicy ?? 'ask') === 'ask'" class="settings-row">
          <div class="min-w-0 flex-1">
            <div class="settings-label">{{ t('transfer.settings.approvalTimeout') }}</div>
            <div class="settings-desc">10–600</div>
          </div>
          <div class="flex items-center gap-2 flex-shrink-0">
            <button class="ft-step-btn" @click="stepTimeout(-10)">−</button>
            <input
              v-model="timeoutInput"
              class="ft-timeout-input"
              type="number"
              min="10"
              max="600"
              inputmode="numeric"
              @focus="syncTimeoutInput()"
              @blur="commitTimeout()"
              @keyup.enter="($event.target as HTMLInputElement).blur()"
            />
            <button class="ft-step-btn" @click="stepTimeout(10)">+</button>
          </div>
        </div>
      </div>
    </section>

    <!-- ==================== 明文安全告知（spec §10） ==================== -->
    <div class="ft-warning-box">
      <p class="ft-warning-text">
        {{ t('transfer.settings.plainWarning') }}
      </p>
    </div>
  </div>
</template>

<style scoped>
/* 设置区次级说明文字 */
.ft-settings-hint {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 设置按钮流式字号 */
.ft-settings-btn {
  font-size: clamp(0.75rem, 0.8125rem + (100vw - 360px) / 800, 0.875rem);
  font-weight: 500;
}

/* 共享目录条目 URI（次级信息行） */
.ft-root-uri {
  margin-top: 0.125rem;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  color: var(--mobile-text-muted);
}

/* 免授权特殊条目徽标 */
.ft-free-badge {
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  font-weight: 500;
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  color: var(--mobile-text-secondary);
}

/* 失效条目徽标（授权被回收/目录被删） */
.ft-invalid-badge {
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  font-weight: 500;
  color: var(--mobile-warning);
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 8%, transparent);
}

/* 重新授权按钮：警示色描边，44px 触控目标 */
.ft-reauth-btn {
  min-height: 2.25rem;
  padding: 0 0.625rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  color: var(--mobile-warning);
  border: 1px solid var(--mobile-warning-muted);
  background: transparent;
  transition: opacity 0.15s ease;
}

.ft-reauth-btn:active {
  opacity: 0.8;
}

.ft-reauth-btn:disabled {
  opacity: 0.5;
}

/* 删除按钮 */
.ft-settings-remove-btn {
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-error);
  border: 1px solid var(--mobile-error-muted);
  background: transparent;
  transition: opacity 0.15s ease;
}

.ft-settings-remove-btn:active {
  opacity: 0.8;
}

/* 步进按钮（并发数）：44px 触控目标 */
.ft-step-btn {
  width: 2.75rem;
  height: 2.75rem;
  border-radius: 0.625rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  font-size: var(--font-size-xl);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s ease;
}

.ft-step-btn:active {
  opacity: 0.8;
}

/* 步进数值：独立 chip 背景，避免「孤儿数字」感 */
.ft-step-value {
  width: 2.5rem;
  height: 2.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 0.625rem;
  background: var(--mobile-bg-tertiary);
  text-align: center;
  font-size: clamp(1rem, 1.0625rem + (100vw - 360px) / 800, 1.125rem);
  font-weight: 600;
  color: var(--mobile-text-primary);
  font-variant-numeric: tabular-nums;
}

/* 下载目录未设置：占位 chip 样式，明确「尚未配置」而非可编辑输入 */
.ft-settings-unset {
  display: inline-flex;
  align-items: center;
  padding: 0.25rem 0.625rem;
  border-radius: 0.5rem;
  background: var(--mobile-bg-tertiary);
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
}

/* 接收策略行：分段控件与说明同行（窄屏允许换行） */
.ft-policy-row {
  flex-wrap: wrap;
  gap: 0.75rem;
}

/* 自绘分段控件（禁原生 select）：胶囊容器 + 激活项 accent tint */
.ft-segmented {
  display: inline-flex;
  padding: 0.1875rem;
  border-radius: 0.625rem;
  background: var(--mobile-bg-tertiary);
  gap: 0.1875rem;
}

.ft-segmented-item {
  min-height: 2.5rem;
  padding: 0 0.75rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  color: var(--mobile-text-secondary);
  background: transparent;
  transition: background-color 0.15s ease, color 0.15s ease;
  -webkit-tap-highlight-color: transparent;
  white-space: nowrap;
}

.ft-segmented-item:active {
  opacity: 0.8;
}

.ft-segmented-item--active {
  background: var(--mobile-bg-elevated);
  color: var(--mobile-accent);
}

/* 同意超时数字输入：完全自绘外观（token 边框/圆角/字号，无系统控件观感） */
.ft-timeout-input {
  width: 4.5rem;
  height: 2.75rem;
  border-radius: 0.625rem;
  border: 1px solid var(--mobile-border);
  background: var(--mobile-bg-elevated);
  color: var(--mobile-text-primary);
  text-align: center;
  font-size: clamp(0.875rem, 0.9375rem + (100vw - 360px) / 800, 1rem);
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  transition: border-color 0.15s ease;
  -webkit-appearance: none;
  appearance: none;
  -moz-appearance: textfield;
}

.ft-timeout-input:focus {
  outline: none;
  border-color: var(--mobile-accent);
}

.ft-timeout-input::-webkit-outer-spin-button,
.ft-timeout-input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

/* 黄色提醒框（使用说明 / 安全告知共用） */
.ft-warning-box {
  padding: 0.75rem 1rem;
  border-radius: 0.75rem;
  border: 1px solid var(--mobile-warning-muted);
  background: color-mix(in srgb, var(--mobile-warning) 6%, transparent);
}

.ft-warning-text {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  line-height: 1.5;
  color: var(--mobile-warning);
  margin: 0;
}
</style>
