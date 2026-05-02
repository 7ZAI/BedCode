<template>
  <div class="output-renderer h-full overflow-auto" ref="containerRef">
    <!-- Mode Toggle -->
    <div class="sticky top-0 z-10 bg-dark-900/90 backdrop-blur px-3 py-2 flex gap-2 border-b border-dark-700">
      <button
        :class="[
          'px-3 py-1.5 rounded-lg text-sm font-medium transition-colors',
          mode === 'enhanced' ? 'bg-primary-600 text-white' : 'bg-dark-700 text-dark-300'
        ]"
        @click="mode = 'enhanced'"
      >
        增强
      </button>
      <button
        :class="[
          'px-3 py-1.5 rounded-lg text-sm font-medium transition-colors',
          mode === 'raw' ? 'bg-primary-600 text-white' : 'bg-dark-700 text-dark-300'
        ]"
        @click="mode = 'raw'"
      >
        原始
      </button>
    </div>

    <!-- Enhanced Mode -->
    <div v-if="mode === 'enhanced'" class="p-3 space-y-3" :style="{ fontSize: fontSizeStyle }">
      <template v-for="block in blocks" :key="block.id">
        <!-- Text block -->
        <div v-if="block.type === 'text'" class="whitespace-pre-wrap break-words">
          {{ block.content }}
        </div>

        <!-- Markdown block -->
        <div v-else-if="block.type === 'markdown'" class="prose prose-invert prose-sm max-w-none">
          <div v-html="renderMarkdown(block.content)"></div>
        </div>

        <!-- Code block -->
        <div v-else-if="block.type === 'code'" class="bg-dark-800 rounded-lg overflow-hidden">
          <div class="flex items-center justify-between px-3 py-2 bg-dark-700/50">
            <span class="text-xs text-dark-400">{{ block.language || 'code' }}</span>
            <button
              class="text-xs text-dark-400 hover:text-white"
              @click="copyCode(block.content)"
            >
              复制
            </button>
          </div>
          <pre class="p-3 overflow-x-auto"><code>{{ block.content }}</code></pre>
        </div>

        <!-- Error block -->
        <div v-else-if="block.type === 'error'" class="bg-red-900/20 border border-red-800/50 rounded-lg p-3">
          <p class="text-red-400 whitespace-pre-wrap">{{ block.content }}</p>
        </div>

        <!-- Tool use block -->
        <div v-else-if="block.type === 'tool_use'" class="bg-purple-900/20 border border-purple-800/50 rounded-lg p-3">
          <p class="text-purple-400">{{ block.content }}</p>
        </div>

        <!-- Progress block -->
        <div v-else-if="block.type === 'progress'" class="bg-dark-800 rounded-lg p-3">
          <div class="flex items-center gap-3">
            <div class="flex-1 h-2 bg-dark-700 rounded-full overflow-hidden">
              <div
                class="h-full bg-primary-500 transition-all"
                :style="{ width: (block.percent || 0) + '%' }"
              ></div>
            </div>
            <span class="text-xs text-dark-400">{{ block.percent }}%</span>
          </div>
          <p v-if="block.message" class="text-xs text-dark-400 mt-2">{{ block.message }}</p>
        </div>
      </template>

      <!-- Empty state -->
      <div v-if="blocks.length === 0" class="flex items-center justify-center h-32">
        <p class="text-dark-500">等待输出...</p>
      </div>
    </div>

    <!-- Raw Mode -->
    <div v-else class="p-3">
      <pre class="whitespace-pre-wrap break-words font-mono" :style="{ fontSize: fontSizeStyle }">{{ rawOutput }}</pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick, computed } from 'vue'
import type { OutputBlock } from '@/composables/useOutputParser'
import { useSettingsStore } from '@/stores/settings'

const props = defineProps<{
  blocks: OutputBlock[]
  rawOutput: string
  autoScroll?: boolean
}>()

const settingsStore = useSettingsStore()

const fontSizeStyle = computed(() => {
  return `${settingsStore.settings.ui.terminal_font_size}px`
})

const mode = ref<'enhanced' | 'raw'>('enhanced')
const containerRef = ref<HTMLElement | null>(null)

// Auto scroll to bottom when new content arrives
watch(() => props.blocks.length, async () => {
  if (props.autoScroll !== false) {
    await nextTick()
    if (containerRef.value) {
      containerRef.value.scrollTop = containerRef.value.scrollHeight
    }
  }
})

function renderMarkdown(content: string): string {
  // Simple markdown rendering
  let html = content
    // Headers
    .replace(/^### (.+)$/gm, '<h3 class="text-lg font-semibold mt-4 mb-2">$1</h3>')
    .replace(/^## (.+)$/gm, '<h2 class="text-xl font-semibold mt-4 mb-2">$1</h2>')
    .replace(/^# (.+)$/gm, '<h1 class="text-2xl font-bold mt-4 mb-2">$1</h1>')
    // Bold
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    // Italic
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    // Code inline
    .replace(/`(.+?)`/g, '<code class="bg-dark-700 px-1 rounded">$1</code>')
    // Lists
    .replace(/^- (.+)$/gm, '<li class="ml-4">$1</li>')
    .replace(/^\d+\. (.+)$/gm, '<li class="ml-4">$1</li>')
    // Paragraphs
    .replace(/\n\n/g, '</p><p class="my-2">')

  return `<p class="my-2">${html}</p>`
}

function copyCode(code: string) {
  navigator.clipboard.writeText(code)
  // Could emit event for toast notification
}
</script>

<style scoped>
.output-renderer {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.output-renderer pre,
.output-renderer code {
  font-family: v-bind('settingsStore.settings.ui.terminal_font_family'), 'SF Mono', 'Fira Code', 'Consolas', monospace;
}
</style>
