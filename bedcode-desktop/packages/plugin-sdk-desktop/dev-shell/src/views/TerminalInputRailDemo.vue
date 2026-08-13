<script setup lang="ts">
/**
 * TerminalInputRail 可视化测试页（dev-shell 专用调试视图）
 *
 * 模拟终端场景验证导航条组件（SDK 源码直引，非副本）：
 * - 左侧模拟终端：滚动容器 + 伪 buffer 行（`$ 命令` 行 + 输出行）
 * - 提交输入 → 追加一条 marker（line = 提交时 buffer 尾部行）并补几行输出
 * - 右侧渲染真实 TerminalInputRail：验证横线出现、位置映射、hover 展开、点击导航
 * - navigate 事件 → 滚动模拟终端到对应行（模拟真实 scrollToLine 效果）
 */
import { nextTick, ref } from 'vue'
import TerminalInputRail, { type InputMarker } from '../../../src/ui/TerminalInputRail.vue'

const markers = ref<InputMarker[]>([])
const bufferLength = ref(0)
const isAltBuffer = ref(false)
const nextId = ref(1)
const inputText = ref('')
const scrollEl = ref<HTMLElement | null>(null)
const navLog = ref('')
const displayLines = ref<string[]>([])

/** 提交一次输入：marker line = 当前 buffer 尾部（模拟真实终端输入行位置），随后补输出行 */
function addMarker() {
  const text = inputText.value.trim() || `示例命令 ${nextId.value}`
  const line = bufferLength.value
  markers.value.push({ id: nextId.value++, line, text })
  if (markers.value.length > 10) markers.value.shift()

  pushLine(`$ ${text}`)
  pushLine(`输出行 ${line}：处理完成`)
  pushLine('✔ 退出码 0')
  bufferLength.value += 3
  inputText.value = ''
  scrollToBottom()
  navLog.value = `记录 marker: line=${line} text="${text}"`
}

function pushLine(text: string) {
  displayLines.value.push(text)
}

function scrollToBottom() {
  nextTick(() => {
    const el = scrollEl.value
    if (el) el.scrollTop = el.scrollHeight
  })
}

function clearAll() {
  markers.value = []
  displayLines.value = []
  bufferLength.value = 0
  navLog.value = ''
}

/** 模拟真实 scrollToLine：按 line/bufferLength 比例滚动容器 */
function handleNavigate(line: number) {
  const el = scrollEl.value
  if (el && bufferLength.value > 1) {
    const maxScroll = el.scrollHeight - el.clientHeight
    el.scrollTop = (line / (bufferLength.value - 1)) * maxScroll
  }
  const hit = markers.value.find((m) => m.line === line)
  navLog.value = `navigate → buffer line ${line}（${hit ? `$ ${hit.text}` : '未找到'}）`
}
</script>

<template>
  <div class="h-full flex flex-col bg-page">
    <!-- 调试控制栏 -->
    <div class="flex items-center gap-3 px-4 py-2 border-b border-[var(--border)] flex-shrink-0">
      <h2 class="text-sm font-medium text-[var(--text-primary)]">TerminalInputRail 测试</h2>
      <span class="text-xs text-[var(--text-tertiary)]">
        markers: {{ markers.length }}（最多 10）· bufferLength: {{ bufferLength }} · altBuffer: {{ isAltBuffer }}
      </span>
      <span class="flex-1" />
      <button class="chip" @click="addMarker()">+ 输入</button>
      <button class="chip" @click="clearAll()">清屏</button>
      <button class="chip" @click="isAltBuffer = !isAltBuffer">切换 alt buffer</button>
    </div>

    <div class="flex-1 min-h-0 flex flex-col">
      <!-- 模拟终端区域：relative 容器 + 右侧真实导航条 -->
      <div class="flex-1 min-h-0 relative overflow-hidden bg-[#1e1e2e]">
        <div
          ref="scrollEl"
          class="absolute inset-0 overflow-y-auto p-4 font-mono text-xs leading-relaxed text-[#f8f8f2]"
        >
          <div
            v-for="(l, i) in displayLines"
            :key="i"
            :class="l.startsWith('$ ') ? 'text-[#50fa7b]' : ''"
          >{{ l }}</div>
          <div v-if="!displayLines.length" class="text-[#6272a4]">
            （空终端 — 提交输入后右侧出现主题色横线，鼠标移入展开列表）
          </div>
        </div>

        <!-- 被测组件：宿主源码，非副本 -->
        <TerminalInputRail
          :markers="markers"
          :buffer-length="bufferLength"
          :is-alt-buffer="isAltBuffer"
          @navigate="handleNavigate"
        />
      </div>

      <!-- 输入区 -->
      <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2 border-t border-[var(--border)]">
        <input
          v-model="inputText"
          class="flex-1 min-w-0 bg-[var(--bg-input)] border border-[var(--border-input)] rounded-input px-3 py-2 text-xs text-[var(--text-primary)] focus:border-[var(--color-primary)] outline-none"
          placeholder="输入命令文本，回车提交（作为一次输入记录）"
          @keydown.enter="addMarker()"
        />
        <button class="chip chip-primary" @click="addMarker()">提交输入</button>
      </div>

      <!-- navigate 调试日志 -->
      <div
        v-if="navLog"
        class="flex-shrink-0 px-4 py-1.5 border-t border-[var(--border)] text-xs text-[var(--color-primary)]"
      >
        {{ navLog }}
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
  cursor: pointer;
  transition: all 0.2s;
}
.chip:hover {
  color: var(--text-primary);
  border-color: var(--border-strong);
}
.chip-primary {
  color: var(--color-primary);
  border-color: var(--color-primary);
  background: var(--color-primary-light);
}
</style>
