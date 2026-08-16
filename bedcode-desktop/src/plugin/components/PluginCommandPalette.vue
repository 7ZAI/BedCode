<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-50 flex items-start justify-center pt-[20vh]"
      @click.self="close"
    >
      <div class="bg-card rounded-card shadow-2xl border border-[var(--border)] w-full max-w-md overflow-hidden">
        <div class="p-3 border-b border-[var(--border)]">
          <input
            ref="searchInput"
            v-model="query"
            class="w-full bg-transparent text-sm outline-none text-[var(--text-primary)] placeholder-[var(--text-tertiary)]"
            :placeholder="$t('desktop.plugin.searchCommands')"
            @keydown.escape="close"
          />
        </div>
        <ul class="max-h-64 overflow-y-auto">
          <li
            v-for="cmd in filteredCommands"
            :key="`${cmd.plugin_id}:${cmd.command_id}`"
            class="px-4 py-2 cursor-pointer hover:bg-[var(--bg-hover)] text-sm text-[var(--text-secondary)]"
            @click="executeCommand(cmd)"
          >
            {{ cmd.title }}
          </li>
          <li v-if="filteredCommands.length === 0" class="px-4 py-3 text-sm text-[var(--text-tertiary)] text-center">
            {{ $t('desktop.plugin.noCommands') }}
          </li>
        </ul>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * PluginCommandPalette — 插件命令面板 (Ctrl+Shift+P)
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { pluginListCommands, pluginInvoke, type CommandEntry } from '../commands'
import { pluginLoader } from '../loader'

const visible = ref(false)
const query = ref('')
const commands = ref<CommandEntry[]>([])
const searchInput = ref<HTMLInputElement | null>(null)

const filteredCommands = computed(() => {
  if (!query.value) return commands.value
  const q = query.value.toLowerCase()
  return commands.value.filter(c => c.title.toLowerCase().includes(q))
})

function open() {
  visible.value = true
  query.value = ''
  loadCommands()
  setTimeout(() => searchInput.value?.focus(), 50)
}

function close() {
  visible.value = false
}

async function loadCommands() {
  try {
    commands.value = await pluginListCommands()
  } catch {
    commands.value = []
  }
}

async function executeCommand(cmd: CommandEntry) {
  close()

  // 前端 TS 插件：从 PluginLoader 查找本地 commandHandlers
  const activePlugin = pluginLoader.getActivePlugin(cmd.plugin_id)
  if (activePlugin) {
    try {
      await activePlugin.context.commands.execute(cmd.command_id)
      return
    } catch {
      // 前端未找到 handler，尝试 Rust 端
    }
  }

  // Rust 插件（WASM）：通过 pluginInvoke 路由到 PluginHost
  try {
    await pluginInvoke(cmd.plugin_id, cmd.command_id)
  } catch (e: any) {
    console.error(`[PluginCommandPalette] Failed to execute ${cmd.command_id}:`, e)
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.shiftKey && e.key === 'P') {
    e.preventDefault()
    if (visible.value) {
      close()
    } else {
      open()
    }
  }
}

onMounted(() => {
  window.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})
</script>
