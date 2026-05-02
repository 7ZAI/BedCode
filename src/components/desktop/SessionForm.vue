<template>
  <form @submit.prevent="handleSubmit" class="space-y-4">
    <!-- Name -->
    <Input
      v-model="form.name"
      label="名称"
      placeholder="会话名称"
      required
    />

    <!-- Environment -->
    <Select
      v-model="form.environment"
      label="执行环境"
      :options="environmentOptions"
      required
    />

    <!-- WSL Distribution -->
    <Select
      v-if="form.environment === 'wsl2'"
      v-model="form.wslDistro"
      label="WSL 发行版"
      :options="wslDistroOptions"
      placeholder="选择发行版"
      required
    />

    <!-- Working Directory -->
    <Input
      v-model="form.workingDir"
      label="工作目录"
      placeholder="C:\Users\..."
      required
    >
      <template #suffix>
        <button
          type="button"
          @click="browseDir"
          class="text-primary-400 hover:text-primary-300"
        >
          浏览
        </button>
      </template>
    </Input>

    <!-- Command -->
    <Input
      v-model="form.command"
      label="启动命令"
      placeholder="claude"
      required
      help="输入要执行的命令，如 claude、npm run dev 等"
    />

    <!-- Tmux Session -->
    <Input
      v-model="form.tmuxSession"
      label="Tmux 会话 (可选)"
      placeholder="留空则新建会话"
      help="输入已存在的 Tmux 会话名，或留空创建新会话"
    />

    <!-- Auto Start -->
    <Toggle
      v-model="form.autoStart"
      label="开机自动启动"
    />

    <!-- Actions -->
    <div class="flex justify-end gap-3 pt-4 border-t border-dark-700">
      <Button type="button" variant="secondary" @click="$emit('cancel')">
        取消
      </Button>
      <Button type="submit" variant="primary">
        {{ config ? '保存' : '创建' }}
      </Button>
    </div>
  </form>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import type { SessionConfig } from '@/stores/session'
import Input from '@/components/common/Input.vue'
import Select from '@/components/common/Select.vue'
import Toggle from '@/components/common/Toggle.vue'
import Button from '@/components/common/Button.vue'
import { useWsl } from '@/composables/useTauri'
import { open } from '@tauri-apps/plugin-dialog'
import { useSettingsStore } from '@/stores/settings'

const props = defineProps<{
  config?: SessionConfig | null
}>()

const emit = defineEmits<{
  (e: 'save', form: SessionFormData): void
  (e: 'cancel'): void
}>()

interface SessionFormData {
  name: string
  environment: string
  wslDistro: string
  workingDir: string
  command: string
  tmuxSession: string
  autoStart: boolean
}

const { distros, loadDistros, isAvailable } = useWsl()
const settingsStore = useSettingsStore()

const form = ref<SessionFormData>({
  name: '',
  environment: 'windows',
  wslDistro: '',
  workingDir: '',
  command: 'claude',
  tmuxSession: '',
  autoStart: false,
})

const environmentOptions = [
  { value: 'windows', label: 'Windows 原生' },
  { value: 'wsl2', label: 'WSL2' },
]

const wslDistroOptions = ref<Array<{ value: string; label: string }>>([])

watch(() => props.config, (config) => {
  // 每次配置变化时更新表单
  if (config) {
    // 编辑模式：使用配置的值
    form.value = {
      name: config.name,
      environment: config.environment,
      wslDistro: config.wslDistro || '',
      workingDir: config.workingDir,
      command: config.command,
      tmuxSession: config.tmuxSession || '',
      autoStart: config.autoStart,
    }
  } else {
    // 创建模式：使用默认设置
    form.value = {
      name: '',
      environment: settingsStore.settings.session.default_environment || 'windows',
      wslDistro: settingsStore.settings.session.default_wsl_distro || '',
      workingDir: settingsStore.settings.session.default_working_dir || '',
      command: settingsStore.settings.session.default_command || 'claude',
      tmuxSession: '',
      autoStart: false,
    }
  }
}, { immediate: true })

watch(() => form.value.environment, async (env) => {
  if (env === 'wsl2' && isAvailable.value && distros.value.length === 0) {
    await loadDistros()
    wslDistroOptions.value = distros.value.map(d => ({
      value: d.name,
      label: d.name,
    }))
  }
})

onMounted(async () => {
  // 加载 WSL 发行版列表
  await loadDistros()
  wslDistroOptions.value = distros.value.map(d => ({
    value: d.name,
    label: d.name,
  }))
})

async function browseDir() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: form.value.workingDir || undefined,
    })
    if (selected) {
      form.value.workingDir = selected as string
    }
  } catch (e) {
    console.error('Failed to browse directory:', e)
  }
}

function handleSubmit() {
  emit('save', form.value)
}
</script>
