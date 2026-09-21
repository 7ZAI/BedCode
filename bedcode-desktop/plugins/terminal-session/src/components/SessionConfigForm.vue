<template>
  <form class="space-y-5" @submit.prevent="handleSubmit">
    <!-- Name -->
    <TextInput
      v-model="form.name"
      :label="t('session.form.name')"
      :placeholder="t('session.form.namePlaceholder')"
      required
    />

    <!-- Environment -->
    <Select
      v-model="form.environment"
      :label="t('session.form.environment')"
      :options="environmentOptions"
      required
    />

    <!-- WSL Distribution -->
    <div v-if="form.environment === 'wsl2'">
      <Select
        v-if="!wslLoading"
        v-model="form.wslDistro"
        :label="t('session.form.wslDistro')"
        :options="wslDistroOptions"
        :placeholder="t('session.form.wslDistroPlaceholder')"
        :disabled="wslDistroOptions.length === 0"
        required
      />
      <!-- WSL 枚举中的加载提示（宿主表单同款内联提示） -->
      <div v-else class="form-group">
        <label class="block text-xs font-medium mb-1.5 text-[var(--text-secondary)]">
          {{ t('session.form.wslDistro') }}
          <span class="text-red-500">*</span>
        </label>
        <div
          class="border rounded-input px-4 py-2 border-[var(--border-input)] bg-[var(--bg-input)] text-[var(--text-tertiary)]"
        >
          <span class="text-sm">{{ t('session.form.wslInitializing') }}</span>
        </div>
      </div>
      <!-- 宿主无 WSL：提示而非空下拉（原语显性报错，见 environment 模块） -->
      <p v-if="!wslLoading && wslError" class="mt-1 text-sm text-amber-500">
        {{ t('session.form.wslNotDetected') }}
      </p>
    </div>

    <!-- Working Directory -->
    <TextInput
      v-model="form.workingDir"
      :label="t('session.form.workingDir')"
      placeholder="C:\Users\..."
      required
    >
      <template #suffix>
        <button
          type="button"
          class="text-brand hover:text-[var(--color-primary-hover)]"
          @click="browseDir"
        >
          {{ t('session.button.browse') }}
        </button>
      </template>
    </TextInput>

    <!-- Command -->
    <Select
      v-model="form.commandPreset"
      :label="t('session.form.command')"
      :options="commandPresetOptions"
      :placeholder="t('session.form.commandPlaceholder')"
      required
    />
    <TextInput
      v-if="form.commandPreset === 'custom'"
      v-model="form.command"
      :label="t('session.form.customCommand')"
      placeholder="claude"
      required
      :help="t('session.form.commandHelp')"
    />
  </form>
</template>

<script setup lang="ts">
/**
 * SessionConfigForm — 会话配置表单（宿主 `SessionForm.vue` 的插件版，票 13）
 *
 * 与宿主版行为一致：环境选项按宿主平台过滤（不可用项禁用）、WSL 发行版经宿主
 * 平台事实枚举、工作目录可经系统目录选择器挑选、命令预设与自定义命令互斥。
 *
 * 差异（已在票内记录）：
 * - 「新建默认值」来源：宿主版读宿主设置 `settings.session.default_*`；插件版读
 *   插件存储 `session.formDefaults`（上次成功保存的取值 + 平台默认环境），
 *   宿主设置分组的会话默认值迁移归票 14；
 * - WSL 枚举走插件命令通道 → 宿主 `host-platform.wsl-distros` 原语（插件禁直调
 *   宿主命令面）。
 */
import { computed, inject, onMounted, ref, watch } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import Select from '@binblink/bedcode-plugin-sdk-desktop/ui'
import { platform } from '@tauri-apps/plugin-os'
import TextInput from './TextInput.vue'
import type { SessionConfigDto } from '../composables/useSessionCenter'

/** 新建默认值（视图层从插件存储读取后下传；表单不自行持久化） */
export interface FormDefaults {
  environment?: string
  wslDistro?: string
  workingDir?: string
  command?: string
}

export interface SessionConfigFormData {
  name: string
  environment: string
  wslDistro: string
  workingDir: string
  command: string
  commandPreset: string
  autoStart: boolean
}

const props = defineProps<{
  config?: SessionConfigDto | null
  defaults?: FormDefaults
}>()

const emit = defineEmits<{
  (e: 'save', form: SessionConfigFormData): void
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const presetCommandMap: Record<string, string> = {
  claude: 'claude',
  codex: 'codex',
  pi: 'pi',
  opencode: 'opencode',
}

const form = ref<SessionConfigFormData>({
  name: '',
  // 默认环境在 watch 回调里基于宿主平台覆盖；此处与宿主版同款兜底值
  environment: 'windows',
  wslDistro: '',
  workingDir: '',
  command: 'claude',
  commandPreset: 'claude',
  autoStart: false,
})

// ==================== 平台与执行环境选项 ====================

/**
 * 宿主平台名 —— 经 Tauri OS API 同步取得（plugin-os 由宿主注册；禁以 viewport/UA
 * 推断平台，AGENTS §6）。非 Tauri 环境（dev-shell / 测试）调用会抛错 → null，
 * 与宿主 `usePlatform` 的「探测失败留空」同口径（此时环境选项全禁用、默认 windows）。
 */
const platformName: string | null = (() => {
  try {
    return platform()
  } catch {
    return null
  }
})()

/** 当前平台理论上可用的执行环境白名单（与宿主 useAvailableEnvironments 同规则） */
const availableValues = computed<string[]>(() => {
  if (platformName === 'linux') return ['linux']
  if (platformName === 'windows') return ['windows', 'wsl2']
  return []
})

const environmentOptions = computed(() => {
  const available = new Set(availableValues.value)
  return [
    { value: 'windows', label: t('session.form.windowsNative'), disabled: !available.has('windows') },
    { value: 'wsl2', label: 'WSL2', disabled: !available.has('wsl2') },
    { value: 'linux', label: t('session.form.linuxNative'), disabled: !available.has('linux') },
  ]
})

const defaultEnvironment = computed(() =>
  availableValues.value.length > 0 ? availableValues.value[0] : 'windows',
)

/** 归一化已存环境取值：平台不可用时降级到平台默认（宿主同规则） */
function normalizeEnvironment(stored: string | null | undefined): string {
  if (stored === 'windows' || stored === 'wsl2' || stored === 'linux') {
    if (availableValues.value.includes(stored)) return stored
  }
  return defaultEnvironment.value
}

// ==================== WSL 发行版（宿主平台事实） ====================

const wslDistros = ref<string[]>([])
const wslLoading = ref(false)
const wslError = ref('')

const wslDistroOptions = computed(() =>
  wslDistros.value.map((name) => ({ value: name, label: name })),
)

/** 枚举宿主 WSL 发行版；宿主无 WSL 时原语显性报错 → 置错误态（提示而非空下拉） */
async function loadWslDistros(): Promise<void> {
  wslLoading.value = true
  wslError.value = ''
  try {
    const raw = (await context.commands.execute('session.environment.wsl-distros', {})) as {
      distros?: unknown
    }
    const list = Array.isArray(raw?.distros) ? (raw.distros as string[]) : []
    wslDistros.value = list
  } catch (e) {
    wslDistros.value = []
    wslError.value = (e as Error)?.message || String(e)
    console.error('[Session Center] wsl distros failed:', e)
  } finally {
    wslLoading.value = false
  }
}

// wsl2 分支才需要发行版列表：切到该分支时按需加载一次（失败不重试，避免弹窗内抖动）
watch(
  () => form.value.environment,
  (env) => {
    if (env === 'wsl2' && wslDistros.value.length === 0 && !wslError.value) {
      void loadWslDistros()
    }
  },
)

// ==================== 表单初始化 ====================

const commandPresetOptions = computed(() => [
  { value: 'claude', label: t('session.form.commandPreset.claude') },
  { value: 'codex', label: t('session.form.commandPreset.codex') },
  { value: 'pi', label: t('session.form.commandPreset.pi') },
  { value: 'opencode', label: t('session.form.commandPreset.opencode') },
  { value: 'custom', label: t('session.form.commandPreset.custom') },
])

function presetForCommand(cmd: string): string {
  const entry = Object.entries(presetCommandMap).find(([, c]) => c === cmd)
  return entry ? entry[0] : 'custom'
}

watch(
  () => props.config,
  (config) => {
    if (config) {
      const command = config.command || ''
      // 编辑已有配置：原环境在当前平台不可用时归一化，避免保存后无法启动
      form.value = {
        name: config.name,
        environment: normalizeEnvironment(config.environment),
        wslDistro: config.wslDistro || '',
        workingDir: config.workingDir || '',
        command,
        commandPreset: presetForCommand(command),
        autoStart: config.autoStart ?? false,
      }
    } else {
      const defaults = props.defaults ?? {}
      form.value = {
        name: '',
        environment: defaults.environment
          ? normalizeEnvironment(defaults.environment)
          : defaultEnvironment.value,
        wslDistro: defaults.wslDistro || '',
        workingDir: defaults.workingDir || '',
        command: defaults.command || 'claude',
        commandPreset: presetForCommand(defaults.command || 'claude'),
        autoStart: false,
      }
    }
  },
  { immediate: true },
)

// 选择预设命令时同步填充 command 字段
watch(
  () => form.value.commandPreset,
  (preset) => {
    if (preset !== 'custom') {
      form.value.command = presetCommandMap[preset] ?? ''
    }
  },
)

onMounted(() => {
  if (form.value.environment === 'wsl2') void loadWslDistros()
})

// ==================== 操作 ====================

/** 打开系统目录选择器（宿主同款：@tauri-apps/plugin-dialog 由宿主注册） */
async function browseDir(): Promise<void> {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: form.value.workingDir || undefined,
    })
    if (selected) form.value.workingDir = selected as string
  } catch (e) {
    console.error('[Session Center] browse directory failed:', e)
  }
}

function handleSubmit(): void {
  emit('save', form.value)
}

defineExpose({
  form,
  validate: () => !!form.value.name && !!form.value.workingDir && !!form.value.command,
})
</script>
