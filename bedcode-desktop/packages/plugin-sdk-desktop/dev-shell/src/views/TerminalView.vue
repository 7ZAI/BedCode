<script setup lang="ts">
/**
 * TerminalView — 模拟终端（桌面端）
 *
 * 驱动 mock/session.ts：输入发送（触发 terminal.onInput）、模拟输出（触发 onOutput）、
 * 会话创建/停止、连接/断开。插件注册的终端工具栏项与输入扩展渲染在顶部。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  activeSessionId,
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
import { inputExtensions, terminalToolbarItems } from '../registry'
import { isSvgIcon } from '../utils/icon'

const { t } = useI18n()

const inputText = ref('')
const simulateText = ref('echo hello')
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
</script>

<template>
  <div class="h-full flex flex-col min-h-0">
    <!-- 插件终端工具栏项 + 输入扩展 -->
    <div class="flex items-center gap-2 px-4 py-2 border-b border-[var(--border)] bg-sidebar/60 overflow-x-auto flex-shrink-0">
      <button
        v-for="entry in terminalToolbarItems"
        :key="entry.pluginId + entry.item.id"
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-btn text-xs whitespace-nowrap bg-[var(--color-primary)]/10 text-[var(--color-primary)] hover:bg-[var(--color-primary)]/20 transition-colors duration-200"
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
      <button
        v-for="entry in inputExtensions"
        :key="entry.pluginId + entry.ext.id"
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-btn text-xs whitespace-nowrap text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors duration-200"
        @click="entry.ext.onActivate?.()"
      >
        {{ entry.ext.icon ? entry.ext.icon + ' ' : '' }}{{ entry.ext.label }}
      </button>
    </div>

    <div class="flex-1 min-h-0 flex">
      <!-- 会话列表 -->
      <div class="w-44 flex-shrink-0 border-r border-[var(--border)] p-2 flex flex-col gap-1.5 overflow-y-auto">
        <button
          v-for="s in sessions"
          :key="s.id"
          class="flex items-center gap-2 px-3 py-2 rounded-nav text-xs text-left transition-colors duration-200"
          :class="
            activeSessionId === s.id
              ? 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
          "
          @click="activeSessionId = s.id"
        >
          <span
            class="w-2 h-2 rounded-full flex-shrink-0"
            :class="s.status === 'running' ? 'bg-[var(--color-primary)]' : 'bg-[var(--text-tertiary)]'"
          />
          <span class="truncate min-w-0">{{ s.id }}</span>
        </button>
      </div>

      <!-- 输出 + 控制 -->
      <div class="flex-1 min-h-0 flex flex-col">
        <div class="flex-1 min-h-0 overflow-y-auto p-4 font-mono text-xs leading-relaxed text-[var(--text-secondary)]">
          <p v-for="(line, i) in activeOutputs" :key="'o' + i" class="terminal-output">{{ line }}</p>
          <p v-for="(line, i) in activeInputs" :key="'i' + i" class="terminal-output text-[var(--color-primary)]">
            $ {{ line }}
          </p>
        </div>

        <div class="flex-shrink-0 flex flex-wrap items-center gap-2 px-4 py-2 border-t border-[var(--border)]">
          <span
            class="text-[11px] px-2 py-0.5 rounded-tag"
            :class="connected ? 'bg-[var(--color-primary)]/10 text-[var(--color-primary)]' : 'bg-[var(--text-tertiary)]/20 text-[var(--text-tertiary)]'"
          >
            {{ connected ? t('devshell.terminal.connected') : t('devshell.terminal.disconnected') }}
          </span>
          <button class="chip" @click="createSession()">{{ t('devshell.terminal.createSession') }}</button>
          <button class="chip" :disabled="!activeSession" @click="stopSession(activeSessionId)">{{ t('devshell.terminal.stopSession') }}</button>
          <button class="chip" @click="setConnected(!connected)">{{ connected ? t('devshell.terminal.disconnect') : t('devshell.terminal.connect') }}</button>
        </div>

        <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2 border-t border-[var(--border)]">
          <input
            v-model="inputText"
            class="flex-1 min-w-0 bg-[var(--bg-input)] border border-[var(--border-input)] rounded-input px-3 py-2 text-xs text-[var(--text-primary)] focus:border-[var(--color-primary)] outline-none transition-colors duration-200"
            :placeholder="t('devshell.terminal.inputPlaceholder')"
            @keydown.enter="send()"
          />
          <button
            class="px-3 py-2 rounded-btn bg-[var(--color-primary)] text-white text-xs font-medium hover:opacity-90 transition-opacity duration-200"
            @click="send()"
          >
            {{ t('devshell.terminal.send') }}
          </button>
        </div>

        <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2 border-t border-[var(--border)]">
          <input
            v-model="simulateText"
            class="flex-1 min-w-0 bg-[var(--bg-input)] border border-[var(--border-input)] rounded-input px-3 py-2 text-xs text-[var(--text-primary)] focus:border-[var(--color-primary)] outline-none transition-colors duration-200"
            placeholder="output"
            @keydown.enter="simulate()"
          />
          <button class="chip" @click="simulate()">{{ t('devshell.terminal.simulateOutput') }}</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.chip {
  flex-shrink: 0;
  padding: 4px 10px;
  border-radius: var(--radius-button);
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--bg-hover);
  border: 1px solid var(--border);
  transition: all 0.2s;
}
.chip:hover:not(:disabled) {
  color: var(--text-primary);
  border-color: var(--border-strong);
}
.chip:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
