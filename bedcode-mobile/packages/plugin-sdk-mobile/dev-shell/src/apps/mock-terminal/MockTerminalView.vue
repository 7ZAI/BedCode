<script setup lang="ts">
/**
 * 模拟终端（内置应用运行面）
 *
 * dev-shell 的模拟终端：输入发送（记录到会话输入行）、模拟输出（触发 terminal:output
 * 事件供 openTerminalStream mock 消费）、会话创建/停止、连接/断开、认证成功
 * （触发对应 lifecycle 钩子）。底部展示 mobileApi 任务队列 mock。
 *
 * 归属变更：原先它是壳的一个调试页签（`views/MockTerminalView.vue` + 底部导航一项），
 * 现在降为**内置应用**——自持运行面、经壳的 registerSurface 挂载、与被调试插件走
 * 同一条加载与渲染路径。理由：应用自持界面是新宿主壳的核心形态，预览环境里必须
 * 有一个真实应用在场，否则「壳怎么渲染别人的界面」这件事在 dev-shell 里无法验证。
 *
 * 与真机的差异见 README「Mock 边界」：WASM 后端不在浏览器运行，这里驱动的是
 * mock/session.ts 的模拟会话。
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
} from '../../mock/session'
import { queueTasks } from '../../mock/mobile-api'

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
  <div class="h-full flex flex-col min-h-0 mobile-ui mobile-app">

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
      <span class="text-[var(--mobile-text-muted)]">{{ t('devshell.terminal.queueMock') }}</span>
      <span class="text-[var(--mobile-text-secondary)] truncate min-w-0">
        {{ queueTasks.length ? queueTasks.map((task) => task.prompt).join(' / ') : t('devshell.terminal.queueEmpty') }}
      </span>
      <button class="ml-auto flex-shrink-0 text-[var(--mobile-text-muted)] hover:text-[var(--mobile-error)] transition-colors duration-200" @click="resetQueue()">
        {{ t('devshell.terminal.queueClear') }}
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
  transition: color 0.2s, border-color 0.2s;
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