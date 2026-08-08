<script setup lang="ts">
/**
 * Dev Toolbar — 工作台工具条：被调试插件、主题切换（深色/浅色/跟随系统）、
 * 手机框开关、日志面板开关
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { logs, plugins } from '../registry'
import { useDevTheme, type ThemeMode } from '../theme'

defineProps<{ frame: boolean }>()
defineEmits<{ 'toggle-frame': [] }>()

const { t } = useI18n()
const logOpen = defineModel<boolean>('logOpen', { default: false })
const { theme, setTheme } = useDevTheme()

const themeOptions = computed(() => [
  { value: 'dark' as ThemeMode, label: t('devshell.theme.dark') },
  { value: 'light' as ThemeMode, label: t('devshell.theme.light') },
  { value: 'system' as ThemeMode, label: t('devshell.theme.system') },
])

const summary = computed(() =>
  plugins.value.map((p) => `${p.name}（${p.state}）`).join('，') || '未加载插件',
)
const errorCount = computed(() => logs.value.filter((l) => l.level === 'error').length)
</script>

<template>
  <div
    class="flex items-center gap-2 px-4 h-12 flex-shrink-0 border-b border-white/10 bg-[#1b1b22] text-[13px] text-[#9ca3af]"
  >
    <span class="font-semibold text-[#e5e7eb] whitespace-nowrap">BedCode Dev Shell</span>
    <span class="truncate min-w-0 text-[#6b7280]">{{ summary }}</span>
    <span class="flex-1" />

    <!-- 主题切换（与宿主设置页同款三选项分段按钮） -->
    <div class="flex items-center rounded-md bg-white/5 p-0.5">
      <button
        v-for="opt in themeOptions"
        :key="opt.value"
        class="px-2 py-0.5 rounded text-[12px] transition-colors duration-200"
        :class="
          theme === opt.value
            ? 'bg-[#00d4ff]/20 text-[#00d4ff]'
            : 'text-[#9ca3af] hover:text-[#d1d5db]'
        "
        @click="setTheme(opt.value)"
      >
        {{ opt.label }}
      </button>
    </div>

    <button
      class="px-2.5 py-1 rounded-md bg-white/5 text-[#d1d5db] hover:bg-white/10 transition-colors duration-200"
      @click="$emit('toggle-frame')"
    >
      {{ frame ? t('devshell.frame.on') : t('devshell.frame.off') }}
    </button>
    <button
      class="px-2.5 py-1 rounded-md bg-white/5 hover:bg-white/10 transition-colors duration-200 flex items-center gap-1"
      @click="logOpen = !logOpen"
    >
      <span>{{ t('devshell.logs.title') }}</span>
      <span v-if="errorCount" class="px-1 rounded bg-red-500/20 text-red-400 text-[11px]">{{ errorCount }}</span>
    </button>
  </div>
</template>
