<script setup lang="ts">
/**
 * MockTerminalView — 模拟终端
 *
 * 驱动 mock/session.ts：输入发送（触发插件 onTerminalInput）、模拟输出
 * （触发 onOutput / onTerminalOutput）、会话创建/停止、连接/断开、认证成功
 * （触发对应 lifecycle 钩子）。插件注册的终端工具栏项渲染在顶部。
 * 底部展示 mobileApi 任务队列 mock（auto-task 类插件的调试入口）。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  activeSessionId,
  authSuccess,
  connected,
  createSession,
  inputs,
  outputs,
  sendInputToSession,
  sendOutput,
  sessions,
  setConnected,
  stopSession,
} from '../mock/session'
import { queueTasks } from '../mock/mobile-api'
import { terminalToolbarItems } from '../registry'
import { isSvgIcon } from '../utils/icon'

const { t } = useI18n()

const inputText = ref('')
const simulateText = ref('ls -la')
const activeOutputs = computed(() => outputs[activeSessionId.value] || [])
const activeInputs = computed(() => inputs[activeSessionId.value] || [])
const activeSession = computed(() => sessions.value.find((s) => s.id === activeSessionId.value))

function send() {
  const text = inputText.value
  if (!text.trim()) return
  sendInputToSession(activeSessionId.value, text)
  inputText.value = ''
}

function simulate() {
  sendOutput(activeSessionId.value, simulateText.value || 'mock output')
}

function resetQueue() {
  queueTasks.value = []
}
</script>

<template>
  <div class="h-full flex flex-col min-h-0">
    <!-- 插件终端工具栏项 -->
    <div
      v-if="terminalToolbarItems.length"
      class="flex items-center gap-2 px-4 py-2 border-b border-[var(--mobile-border)] bg-[var(--mobile-bg-secondary)]/60 overflow-x-auto flex-shrink-0"
    >
      <button
        v-for="entry in terminalToolbarItems"
        :key="entry.pluginId + entry.item.id"
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)] text-xs whitespace-nowrap hover:bg-[var(--mobile-accent-secondary)] transition-colors duration-200"
        @click="entry.item.onClick?.()"
      >
        <span v-if="isSvgIcon(entry.item.icon)" class="w-3.5 h-3.5">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
            <path :d="entry.item.icon" />
          </svg>
        </span>
        <span v-else>{{ entry.item.icon || '' }}</span>
        {{ entry.item.label }}
      </button>
    </div>

    <div class="flex-1 min-h-0 flex flex-col md:flex-row">
      <!-- 会话列表 -->
      <div class="md:w-44 flex-shrink-0 border-b md:border-b-0 md:border-r border-[var(--mobile-border)] p-2 flex md:flex-col gap-1.5 overflow-x-auto md:overflow-y-auto">
        <button
          v-for="s in sessions"
          :key="s.id"
          class="flex items-center gap-2 px-3 py-2 rounded-lg text-xs text-left min-w-[120px] transition-colors duration-200"
          :class="
            activeSessionId === s.id
              ? 'bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]'
              : 'text-[var(--mobile-text-secondary)] hover:bg-[var(--mobile-bg-tertiary)]'
          "
          @click="activeSessionId = s.id"
        >
          <span
            class="w-2 h-2 rounded-full flex-shrink-0"
            :class="s.status === 'running' ? 'bg-[var(--mobile-success)]' : 'bg-[var(--mobile-text-disabled)]'"
          />
          <span class="truncate min-w-0">{{ s.id }}</span>
        </button>
      </div>

      <!-- 输出 + 控制 -->
      <div class="flex-1 min-h-0 flex flex-col">
        <div class="flex-1 min-h-0 overflow-y-auto p-3 font-mono text-xs leading-relaxed text-[var(--mobile-text-secondary)]">
          <p v-for="(line, i) in activeOutputs" :key="'o' + i" class="terminal-output">{{ line }}</p>
          <p v-for="(line, i) in activeInputs" :key="'i' + i" class="terminal-output text-[var(--mobile-accent)]">
            $ {{ line }}
          </p>
        </div>

        <!-- 连接状态 + 生命周期按钮 -->
        <div class="flex-shrink-0 flex flex-wrap items-center gap-1.5 px-3 py-2 border-t border-[var(--mobile-border)]">
          <span
            class="text-[11px] px-2 py-0.5 rounded-full"
            :class="
              connected
                ? 'bg-[var(--mobile-success-muted)] text-[var(--mobile-success)]'
                : 'bg-[var(--mobile-error-muted)] text-[var(--mobile-error)]'
            "
          >
            {{ connected ? t('devshell.terminal.connected') : t('devshell.terminal.disconnected') }}
          </span>
          <button class="chip" @click="createSession()">{{ t('devshell.terminal.createSession') }}</button>
          <button class="chip" :disabled="!activeSession" @click="stopSession(activeSessionId)">{{ t('devshell.terminal.stopSession') }}</button>
          <button class="chip" @click="setConnected(!connected)">{{ connected ? t('devshell.terminal.disconnect') : t('devshell.terminal.connect') }}</button>
          <button class="chip" @click="authSuccess()">{{ t('devshell.terminal.authSuccess') }}</button>
        </div>

        <!-- 输入行 -->
        <div class="flex-shrink-0 flex items-center gap-2 px-3 py-2 border-t border-[var(--mobile-border)]">
          <input
            v-model="inputText"
            class="flex-1 min-w-0 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-2 text-xs text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-input-placeholder)] focus:border-[var(--mobile-input-focus)] outline-none transition-colors duration-200"
            :placeholder="t('devshell.terminal.inputPlaceholder')"
            @keydown.enter="send()"
          />
          <button class="px-3 py-2 rounded-lg bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] text-xs font-medium" @click="send()">
            {{ t('devshell.terminal.send') }}
          </button>
        </div>

        <!-- 模拟输出行 -->
        <div class="flex-shrink-0 flex items-center gap-2 px-3 py-2 border-t border-[var(--mobile-border)]">
          <input
            v-model="simulateText"
            class="flex-1 min-w-0 bg-[var(--mobile-input-bg)] border border-[var(--mobile-input-border)] rounded-lg px-3 py-2 text-xs text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-input-placeholder)] focus:border-[var(--mobile-input-focus)] outline-none transition-colors duration-200"
            placeholder="output"
            @keydown.enter="simulate()"
          />
          <button class="chip" @click="simulate()">{{ t('devshell.terminal.simulateOutput') }}</button>
        </div>
      </div>
    </div>

    <!-- 任务队列 mock（mobileApi） -->
    <div class="flex-shrink-0 border-t border-[var(--mobile-border)] px-4 py-2 flex items-center gap-2 text-xs">
      <span class="text-[var(--mobile-text-muted)]">队列(mock):</span>
      <span class="text-[var(--mobile-text-secondary)] truncate min-w-0">
        {{ queueTasks.length ? queueTasks.map((task) => task.prompt).join(' / ') : '空' }}
      </span>
      <button class="ml-auto flex-shrink-0 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-error)] transition-colors duration-200" @click="resetQueue()">
        清空
      </button>
    </div>
  </div>
</template>

<style scoped>
.chip {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: 8px;
  font-size: 11px;
  color: var(--mobile-text-secondary);
  background: var(--mobile-bg-tertiary);
  border: 1px solid var(--mobile-border);
  transition: all 0.2s;
}
.chip:hover:not(:disabled) {
  color: var(--mobile-text-primary);
  border-color: var(--mobile-border-hover);
}
.chip:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
