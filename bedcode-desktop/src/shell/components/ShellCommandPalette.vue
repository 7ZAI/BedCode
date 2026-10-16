<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[70] flex items-start justify-center"
      style="background: color-mix(in srgb, black 45%, transparent)"
      @click.self="close"
    >
      <div
        class="palette w-[min(460px,92vw)] mt-[14vh] flex flex-col bg-[var(--bg-card)] border border-[var(--border-strong)] rounded-[var(--radius-sheet)] shadow-[var(--shadow-card-hover)] overflow-hidden max-h-[58vh]"
        role="dialog"
        aria-label="命令面板"
        @keydown="onKeydown"
      >
        <div class="srch flex items-center gap-2.5 px-3.5 py-2.5 border-b border-[var(--border)]">
          <ShellIcon name="search" :size="15" />
          <input
            ref="inputEl"
            v-model="query"
            class="flex-1 min-w-0 bg-transparent outline-none text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)] placeholder-[var(--text-tertiary)]"
            :placeholder="t('shell.palette.placeholder')"
            aria-label="命令搜索"
          />
        </div>
        <div class="lst flex-1 min-h-0 overflow-y-auto p-1.5">
          <template v-for="(group, gi) in groups" :key="gi">
            <div class="grp">{{ group.label }}</div>
            <button
              v-for="(cmd, i) in group.items"
              :key="cmd.id"
              class="it w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-left text-[calc(12px*var(--ui-scale))] transition-colors"
              :class="isCurrent(gi, i) ? 'cur' : ''"
              @mouseenter="setCurrent(gi, i)"
              @click="run(cmd)"
            >
              <ShellIcon name="terminal" :size="12" />
              <span class="truncate">{{ cmd.title }}</span>
            </button>
          </template>
          <div v-if="!groups.length" class="empty">{{ t('shell.palette.empty') }}</div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ShellCommandPalette — 壳内命令面板（Ctrl+Shift+P / 工具栏放大镜）
 *
 * 数据 = 宿主动作（主屏 / 系统页 / 启动屏重演等）+ 插件命令（plugin_list_commands）。
 * 插件命令执行与既有 PluginCommandPalette 同口径：TS 插件走 context.commands.execute，
 * Rust（WASM）插件走 pluginInvoke + 宿主凭证。
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ensureHostCredential,
  pluginInvoke,
  pluginListCommands,
  type CommandEntry,
} from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import { logger } from '@/utils/frontendLogger'
import { useShellState } from '../useShellState'
import ShellIcon from './ShellIcon.vue'

const { t } = useI18n()
const state = useShellState()

const open = computed(() => state.paletteOpen.value)
const query = ref('')
const pluginCommands = ref<CommandEntry[]>([])
const currentIdx = ref(0)
const inputEl = ref<HTMLInputElement | null>(null)

/** 宿主动作（壳的机制面入口；执行直接映射到壳状态动作） */
interface HostCmd {
  id: string
  titleKey: string
  run: () => void
}
const HOST_CMDS: HostCmd[] = [
  { id: 'host.home', titleKey: 'shell.palette.hostHome', run: () => state.goHome() },
  {
    id: 'host.settings',
    titleKey: 'shell.palette.hostSettings',
    run: () => state.openSys('settings'),
  },
  { id: 'host.apps', titleKey: 'shell.palette.hostApps', run: () => state.openSys('apps') },
  { id: 'host.perms', titleKey: 'shell.palette.hostPerms', run: () => state.openSys('perms') },
  {
    id: 'host.boot',
    titleKey: 'shell.palette.hostBoot',
    run: () => {
      state.boot.value = true
    },
  },
]

interface PaletteItem {
  id: string
  title: string
  kind: 'host' | 'plugin'
  pluginId?: string
  commandId?: string
  /** 宿主动作执行回调（kind === 'host'） */
  run?: () => void
}

function loadCommands() {
  void pluginListCommands()
    .then((cmds) => {
      pluginCommands.value = cmds
    })
    .catch(() => {
      pluginCommands.value = []
    })
}

const filtered = computed<PaletteItem[]>(() => {
  const q = query.value.trim().toLowerCase()
  const host: PaletteItem[] = HOST_CMDS.map((h) => ({
    id: h.id,
    title: t(h.titleKey),
    kind: 'host',
    run: h.run,
  }))
  const plugins: PaletteItem[] = pluginCommands.value
    .filter((c) => !q || c.title.toLowerCase().includes(q) || c.command_id.includes(q))
    .map((c) => ({
      id: `${c.plugin_id}:${c.command_id}`,
      title: c.title,
      kind: 'plugin',
      pluginId: c.plugin_id,
      commandId: c.command_id,
    }))
  return [...host, ...plugins].filter(
    (c) => !q || c.title.toLowerCase().includes(q) || c.id.includes(q),
  )
})

/** 分组（宿主 / 插件），供模板渲染 */
const groups = computed(() => {
  const host = filtered.value.filter((c) => c.kind === 'host')
  const plugins = filtered.value.filter((c) => c.kind === 'plugin')
  const out: Array<{ label: string; items: PaletteItem[] }> = []
  if (host.length) out.push({ label: t('shell.palette.hostGroup'), items: host })
  if (plugins.length) out.push({ label: t('shell.palette.pluginGroup'), items: plugins })
  return out
})

/** 扁平索引 → 当前项（分组展示下用全局下标定位） */
function flatIndexOf(groupIdx: number, itemIdx: number): number {
  let idx = 0
  for (let g = 0; g < groupIdx; g++) idx += groups.value[g].items.length
  return idx + itemIdx
}
function isCurrent(groupIdx: number, itemIdx: number): boolean {
  return flatIndexOf(groupIdx, itemIdx) === currentIdx.value
}
function setCurrent(groupIdx: number, itemIdx: number) {
  currentIdx.value = flatIndexOf(groupIdx, itemIdx)
}

function close() {
  state.paletteOpen.value = false
}

async function run(cmd: PaletteItem) {
  close()
  if (cmd.kind === 'host') {
    cmd.run?.()
    return
  }
  // 插件命令：TS 插件本地 handler 优先，否则 Rust 端
  const activePlugin = cmd.pluginId ? pluginLoader.getActivePlugin(cmd.pluginId) : null
  if (activePlugin && cmd.commandId) {
    try {
      await activePlugin.context.commands.execute(cmd.commandId)
      return
    } catch {
      // 前端未找到 handler，尝试 Rust 端
    }
  }
  if (cmd.pluginId && cmd.commandId) {
    try {
      await pluginInvoke(cmd.pluginId, cmd.commandId, undefined, await ensureHostCredential())
    } catch (e: any) {
      logger.error(`[ShellPalette] Failed to execute ${cmd.commandId}:`, e)
    }
  }
}

// 打开时聚焦输入框 + 拉命令清单 + 重置光标
watch(open, (v) => {
  if (v) {
    query.value = ''
    currentIdx.value = 0
    loadCommands()
    setTimeout(() => inputEl.value?.focus(), 30)
  }
})

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return
  if (e.key === 'Escape') {
    close()
    return
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    currentIdx.value = Math.min(currentIdx.value + 1, filtered.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    currentIdx.value = Math.max(0, currentIdx.value - 1)
  } else if (e.key === 'Enter') {
    const cmd = filtered.value[currentIdx.value]
    if (cmd) void run(cmd)
  }
}
</script>

<style scoped>
.grp {
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-tertiary);
  padding: 8px 10px 3px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
.it {
  color: var(--text-secondary);
  transition:
    background-color 0.12s,
    color 0.12s;
}
.it:hover,
.it.cur {
  background: var(--color-primary-light);
  color: var(--text-primary);
}
.empty {
  padding: 18px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: calc(12px * var(--ui-scale));
}
</style>
