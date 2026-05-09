<template>
  <div class="terminal h-full overflow-auto" ref="containerRef">
    <div
      class="terminal-output whitespace-pre-wrap break-words font-mono"
      :style="{ fontSize: fontSizeStyle }"
      v-html="renderedHtml"
    ></div>
    <div v-if="!rawOutput" class="flex items-center justify-center h-32 text-dark-500 text-sm">
      等待输出...
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useSettingsStore } from '@/stores/settings'
import { useAnsiRenderer } from '@/composables/useAnsiRenderer'

const props = defineProps<{
  rawOutput: string
  autoScroll?: boolean
}>()

const settingsStore = useSettingsStore()
const { renderToHtml } = useAnsiRenderer()

const fontSizeStyle = computed(() => {
  return `${settingsStore.settings.ui.terminal_font_size}px`
})

const fontFamilyStyle = computed(() => {
  return settingsStore.settings.ui.terminal_font_family || "'SF Mono','Fira Code','Consolas',monospace"
})

const containerRef = ref<HTMLElement | null>(null)

const renderedHtml = computed(() => {
  if (!props.rawOutput) return ''
  return renderToHtml(props.rawOutput)
})

// 自动滚动到底部
watch(() => props.rawOutput.length, async () => {
  if (props.autoScroll !== false) {
    await nextTick()
    if (containerRef.value) {
      containerRef.value.scrollTop = containerRef.value.scrollHeight
    }
  }
})
</script>

<style scoped>
.terminal {
  background: #0f172a;
  padding: 12px 16px;
}

.terminal-output {
  line-height: 1.5;
  color: #e2e8f0;
}

.terminal-output :deep(span) {
  font-family: inherit;
}
</style>
