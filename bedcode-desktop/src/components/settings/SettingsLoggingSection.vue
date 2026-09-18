<template>
  <!-- ==================== LOGGING（仅 dev 构建显示；正式版隐藏，后端功能保留） ==================== -->
  <section v-if="isDev">
    <h3 class="wb-section-title">{{ t('settings.log.title') }}</h3>
    <div
      class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
    >
      <!-- 日志级别：分段控件，点击即时热调（不重启） -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.log.level')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.log.levelDesc') }}
          </p>
        </div>
        <div
          class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
        >
          <button
            v-for="opt in logLevelOptions"
            :key="opt.value"
            class="h-8 px-3 text-xs font-medium transition-colors"
            :class="
              logFileLevel === opt.value
                ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            "
            @click="onLogLevelClick(opt.value)"
          >
            {{ t(opt.label) }}
          </button>
        </div>
      </div>

      <!-- 日志格式：text/json，保存后重启生效 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.log.format')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.log.formatDesc') }}
          </p>
        </div>
        <div
          class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
        >
          <button
            v-for="opt in logFormatOptions"
            :key="opt.value"
            class="h-8 px-3 text-xs font-medium transition-colors"
            :class="
              logFormat === opt.value
                ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            "
            @click="logFormat = opt.value"
          >
            {{ t(opt.label) }}
          </button>
        </div>
      </div>

      <!-- 保留文件数：按天轮转保留数量，保存后重启生效 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.log.maxFiles')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.log.maxFilesDesc') }}
          </p>
        </div>
        <input
          type="number"
          :value="logMaxFiles"
          class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="logMaxFiles = Number(($event.target as HTMLInputElement).value)"
        />
      </div>

      <!-- 容量上限（MB）：保存后重启生效 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.log.capacityMb')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.log.capacityMbDesc') }}
          </p>
        </div>
        <input
          type="number"
          :value="logCapacityMb"
          class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
          @input="logCapacityMb = Number(($event.target as HTMLInputElement).value)"
        />
      </div>

      <!-- 打开日志目录 + 保存配置 -->
      <div class="px-5 py-3.5 flex items-center justify-between gap-4">
        <div>
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
            t('settings.log.persist')
          }}</span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.log.persistDesc') }}
          </p>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button class="wb-btn-ghost" @click="onOpenLogDir">
            {{ t('settings.log.openDir') }}
          </button>
          <button class="wb-btn-primary" :disabled="logSaving" @click="onSaveLogConfig">
            {{ t('settings.log.save') }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 日志分组（SettingsView 拆分产物）
 *
 * 仅 dev 构建展示（正式版隐藏，后端功能保留）。日志级别即时热调，
 * format / rotation / max_files / capacity 保存后重启生效。
 */
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import { useLogSettings } from '@/composables/useLogSettings'
import { logger } from '@/utils/frontendLogger'
import { invoke } from '@/utils/invoke'
import i18n from '@/locales'

const { t } = useI18n()
const toast = useToast()

// ==================== 日志设置（desktop-logging-overhaul 04） ====================
// 仅 dev 构建展示（正式版用户不需要关心日志；后端配置能力保留，见 useLogSettings）
const isDev = import.meta.env.DEV
const { setLogLevel, openLogDir, saveLogSettings } = useLogSettings()

const logLevelOptions = [
  { value: 'debug', label: 'settings.log.levelDebug' },
  { value: 'info', label: 'settings.log.levelInfo' },
  { value: 'warn', label: 'settings.log.levelWarn' },
  { value: 'error', label: 'settings.log.levelError' },
]
const logFormatOptions = [
  { value: 'text', label: 'settings.log.formatText' },
  { value: 'json', label: 'settings.log.formatJson' },
]
const logFileLevel = ref('info')
const logFormat = ref('text')
const logMaxFiles = ref(7)
const logCapacityMb = ref(512)
const logSaving = ref(false)

/** 从现有 AppConfig 的 log 段初始化（与 store 分离：store 的 Settings 类型不含 log 层） */
async function loadLogSettings() {
  try {
    const cfg = await invoke<{
      log: {
        file_level: string
        format: string
        max_files: number
        capacity_bytes: number
      }
    }>('get_app_settings')
    logFileLevel.value = cfg.log?.file_level || 'info'
    logFormat.value = cfg.log?.format || 'text'
    logMaxFiles.value = cfg.log?.max_files ?? 7
    logCapacityMb.value = Math.round((cfg.log?.capacity_bytes ?? 512 * 1024 * 1024) / (1024 * 1024))
  } catch (e) {
    logger.error('[Settings] Failed to load log settings:', e)
  }
}

/** 日志级别即时热调（不重启；失败回显原级别并提示） */
async function onLogLevelClick(level: string) {
  const previous = logFileLevel.value
  logFileLevel.value = level
  try {
    await setLogLevel(level)
    toast.success(i18n.global.t('settings.log.levelApplied'))
  } catch (e) {
    logger.error('[Settings] set_log_level failed:', e)
    logFileLevel.value = previous
    toast.error(i18n.global.t('settings.log.saveFailed'))
  }
}

async function onOpenLogDir() {
  try {
    await openLogDir()
  } catch (e) {
    logger.error('[Settings] open_log_dir failed:', e)
    toast.error(i18n.global.t('settings.log.saveFailed'))
  }
}

/** 持久化日志配置（format/rotation/max_files/capacity 重启生效） */
async function onSaveLogConfig() {
  logSaving.value = true
  try {
    await saveLogSettings({
      fileLevel: logFileLevel.value,
      rotation: 'daily',
      maxFiles: logMaxFiles.value,
      format: logFormat.value,
      capacityBytes: logCapacityMb.value * 1024 * 1024,
      consoleInRelease: false,
    })
    toast.success(i18n.global.t('settings.log.saved'))
  } catch (e) {
    logger.error('[Settings] save log settings failed:', e)
    toast.error(i18n.global.t('settings.log.saveFailed'))
  } finally {
    logSaving.value = false
  }
}

onMounted(() => {
  loadLogSettings()
})
</script>
