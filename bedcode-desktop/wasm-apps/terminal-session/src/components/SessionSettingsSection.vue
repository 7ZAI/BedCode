<template>
  <!-- 分组正文：外层 <section> 与标题由宿主统一渲染（settings section 扩展点约定） -->
  <div
    class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
  >
    <!-- 默认执行环境：分段控件（仅展示当前宿主平台可用项） -->
    <div class="px-5 py-3.5 flex items-center justify-between gap-4">
      <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
        t('session.settings.defaultEnvironment')
      }}</span>
      <div
        class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
      >
        <button
          v-for="opt in availableEnvironmentOptions"
          :key="opt.value"
          type="button"
          class="h-8 px-4 text-xs font-medium wb-mono transition-colors"
          :class="
            defaultEnvironment === opt.value
              ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
          "
          @click="saveEnvironment(opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>

    <!-- 默认启动命令（新建会话表单的默认值） -->
    <div class="px-5 py-3.5 flex items-center justify-between gap-4">
      <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
        t('session.settings.defaultCommand')
      }}</span>
      <input
        type="text"
        :value="defaultCommand"
        placeholder="claude"
        class="h-8 w-56 px-2.5 rounded-[6px] wb-mono bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
        @change="saveCommand(($event.target as HTMLInputElement).value)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionSettingsSection — 设置页「会话」分组正文（宿主 `SettingsSessionSection.vue`
 * 的插件版）
 *
 * 归属迁移：会话业务域（创建 / 配置 / 默认值）全部在本插件，宿主那份设置分组
 * 改的是宿主 `settings.session.default_*`——**没有任何消费方**（插件创建会话只读
 * 自己的存储），属失效 UI。故该分组随域下沉：写面 = 插件存储 `session.formDefaults`
 * （与 `SessionCenterView` 交给新建表单的默认值同一份，键名同源），真正被新建会话
 * 表单消费。
 *
 * 平台判定：经 `@tauri-apps/plugin-os` 的 `platform()`（禁 viewport/UA 推断，
 * AGENTS §6），白名单规则与 `SessionConfigForm` 内保持一致（单源在此文件与表单
 * 各自的 computed，语义同源）。
 *
 * 写入口径：环境点击即写；命令用 `change`（失焦/回车）写。写前先读回已有值再
 * 合并——`formDefaults` 还承载上次成功保存的 wslDistro / workingDir，直接覆盖会
 * 抹掉它们。
 */
import { computed, inject, onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'
import { platform } from '@tauri-apps/plugin-os'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'

interface FormDefaults {
  environment?: string
  wslDistro?: string
  workingDir?: string
  command?: string
}

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const FORM_DEFAULTS_KEY = 'session.formDefaults'

const defaults = ref<FormDefaults>({})

/** 宿主平台名（非 Tauri 环境如测试/dev-shell 取不到 → null，环境选项全禁用） */
const platformName: string | null = (() => {
  try {
    return platform()
  } catch {
    return null
  }
})()

const availableValues = computed<string[]>(() => {
  if (platformName === 'linux') return ['linux']
  if (platformName === 'windows') return ['windows', 'wsl2']
  return []
})

const environmentOptions = computed(() => [
  { value: 'windows', label: t('session.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
  { value: 'linux', label: t('session.form.linuxNative') },
])

/** 仅展示当前宿主平台可用项（与宿主原分组同口径：不可用项不出现） */
const availableEnvironmentOptions = computed(() =>
  environmentOptions.value.filter((opt) => availableValues.value.includes(opt.value)),
)

/** 已存环境在当前平台不可用时回落平台默认（老数据兼容） */
const defaultEnvironment = computed(() => {
  const stored = defaults.value.environment
  if (stored && availableValues.value.includes(stored)) return stored
  return availableValues.value[0] ?? 'windows'
})

const defaultCommand = computed(() => defaults.value.command ?? '')

/** 写入一项默认值：读回合并后落插件存储（不抹掉 wslDistro / workingDir） */
async function persist(patch: FormDefaults): Promise<void> {
  const next = { ...defaults.value, ...patch }
  try {
    await context.storage.set(FORM_DEFAULTS_KEY, next)
    defaults.value = next
    toast.success(t('session.settings.saved'))
  } catch (e) {
    console.error('[Session Center] save session settings failed:', e)
    toast.error(t('session.settings.saveFailed'))
  }
}

function saveEnvironment(value: string) {
  void persist({ environment: value })
}

function saveCommand(value: string) {
  const command = value.trim()
  if (command === defaultCommand.value) return
  void persist({ command: command || undefined })
}

onMounted(async () => {
  try {
    const stored = await context.storage.get<FormDefaults>(FORM_DEFAULTS_KEY)
    if (stored && typeof stored === 'object') defaults.value = stored
  } catch (e) {
    // 读取失败只影响预填，不阻断分组渲染
    console.warn('[Session Center] load session settings failed:', e)
  }
})
</script>
