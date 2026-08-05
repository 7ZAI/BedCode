<!--
  PROTOTYPE — 变体 C「素黑」：终端驾驶舱（一次性原型，勿在生产代码引用）

  设计主张：BedCode 的用户是终端重度用户 —— 界面退到最后面，
  信息密度拉满：零卡片、零圆角阴影，一切用 1px 发丝线分组，
  数字全部等宽字体，状态用单字符/短码表达。像 htop，不像 App。
-->
<template>
  <div class="vc h-full flex flex-col bg-[#0c0c0e] text-zinc-200 select-none font-sans">
    <!-- ==================== 全局状态条 ==================== -->
    <div class="flex-shrink-0 flex items-center gap-3 px-4 h-8 border-b border-white/10 font-mono text-[10px] tracking-wider text-zinc-500">
      <span class="flex items-center gap-1.5 text-emerald-400"><span class="w-1.5 h-1.5 bg-emerald-400"></span>LINKED</span>
      <span>{{ currentDevice.address }}</span>
      <span class="flex-1"></span>
      <span>{{ sessions.length }} SESS</span>
      <span>{{ plugins.length }} PLUG</span>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto font-light">
      <!-- ========== 0 连接 ========== -->
      <template v-if="page === 0">
        <div class="px-4 pt-5">
          <div class="flex items-end justify-between">
            <h1 class="page-title">连接</h1>
            <button class="link-btn">发现设备</button>
          </div>

          <!-- 当前连接 -->
          <div class="mt-5">
            <div class="sec-head"><span>ACTIVE LINK</span><span class="font-mono">{{ currentDevice.uptime }}</span></div>
            <div class="divide-y divide-white/[0.06]">
              <div class="kv-row">
                <span class="kv-label">主机</span>
                <span class="kv-value">{{ currentDevice.name }}</span>
              </div>
              <div class="kv-row">
                <span class="kv-label">地址</span>
                <span class="kv-value font-mono text-zinc-400">{{ currentDevice.address }}</span>
              </div>
              <div class="kv-row">
                <span class="kv-label">系统</span>
                <span class="kv-value text-zinc-400">{{ currentDevice.os }}</span>
              </div>
              <div class="kv-row">
                <span class="kv-label">认证</span>
                <span class="kv-value text-emerald-400 font-mono">PAIRED · JWT</span>
              </div>
              <button class="kv-row kv-btn">
                <span class="kv-label">操作</span>
                <span class="kv-value text-red-400 font-mono">DISCONNECT ✕</span>
              </button>
            </div>
          </div>

          <!-- 会话配置 -->
          <div class="mt-6 pb-8">
            <div class="sec-head"><span>SESSION CONFIG</span><span class="font-mono">{{ sessionConfigs.length }}</span></div>
            <div class="divide-y divide-white/[0.06]">
              <div v-for="(c, idx) in sessionConfigs" :key="c.id" class="data-row">
                <span class="row-idx">{{ String(idx + 1).padStart(2, '0') }}</span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-[13px] text-zinc-200 truncate">{{ c.name }}</span>
                    <span class="env-flag" :class="c.env === 'wsl' ? 'flag-wsl' : 'flag-win'">{{ c.env === 'wsl' ? 'WSL' : 'WIN' }}</span>
                  </div>
                  <div class="font-mono text-[10px] text-zinc-600 truncate mt-0.5">{{ c.path }}</div>
                </div>
                <span v-if="c.running" class="state-run font-mono">● RUNNING</span>
                <button v-else class="square-btn">START</button>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 1 会话 ========== -->
      <template v-else-if="page === 1">
        <div class="px-4 pt-5 pb-8">
          <div class="flex items-end justify-between">
            <h1 class="page-title">会话</h1>
            <span class="font-mono text-[10px] text-zinc-600 tracking-wider">{{ runCount }}/{{ sessions.length }} RUNNING</span>
          </div>

          <!-- 表头 -->
          <div class="mt-5 grid grid-cols-[1.5rem_1fr_5rem_3rem] gap-2 pb-2 border-b border-white/15 font-mono text-[9px] tracking-[0.2em] text-zinc-600">
            <span>ID</span><span>NAME / TYPE</span><span class="text-right">ELAPSED</span><span class="text-right">ACT</span>
          </div>
          <div class="divide-y divide-white/[0.06]">
            <div
              v-for="(s, idx) in sessions"
              :key="s.id"
              class="grid grid-cols-[1.5rem_1fr_5rem_3rem] gap-2 items-center py-3 cursor-pointer active:bg-white/[0.03]"
              :class="{ 'opacity-45': s.status === 'stopped' }"
            >
              <span class="row-idx">{{ String(idx + 1).padStart(2, '0') }}</span>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-[13px] text-zinc-200 truncate">{{ s.name }}</span>
                  <span class="state-flag font-mono" :class="'st-' + s.status">{{ stateCode(s.status) }}</span>
                </div>
                <div class="text-[10px] text-zinc-600 mt-0.5">{{ s.type }}<template v-if="s.task"> · {{ s.task }}</template></div>
              </div>
              <span class="text-right font-mono text-xs text-zinc-400 tabular-nums">{{ s.elapsed }}</span>
              <div class="flex justify-end">
                <button v-if="s.status !== 'stopped'" class="stop-btn" title="停止">■</button>
                <button v-else class="del-btn" title="删除">×</button>
              </div>
            </div>
          </div>

          <div class="mt-6 border-t border-white/10 pt-3 font-mono text-[10px] text-zinc-600 tracking-wider">
            HOST {{ currentDevice.name }} · SYNCED 12s AGO
          </div>
        </div>
      </template>

      <!-- ========== 2 工具箱 ========== -->
      <template v-else-if="page === 2">
        <div class="px-4 pt-5 pb-8">
          <h1 class="page-title">工具箱</h1>
          <div class="mt-5">
            <div class="sec-head"><span>TOOLS</span><span class="font-mono">4</span></div>
            <div class="divide-y divide-white/[0.06]">
              <button class="tool-row">
                <span class="row-idx">01</span>
                <span class="flex-1 text-left text-[13px]">预设任务</span>
                <span class="font-mono text-[10px] text-zinc-500 mr-3">LAST 22m AGO</span>
                <span class="badge-num">4</span>
                <span class="arrow">›</span>
              </button>
              <button class="tool-row">
                <span class="row-idx">02</span>
                <span class="flex-1 text-left text-[13px]">Git Glance</span>
                <span class="font-mono text-[10px] text-violet-400/80 mr-3">PLUGIN</span>
                <span class="arrow">›</span>
              </button>
              <button class="tool-row">
                <span class="row-idx">03</span>
                <span class="flex-1 text-left text-[13px]">任务历史</span>
                <span class="arrow">›</span>
              </button>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 3 设置 ========== -->
      <template v-else-if="page === 3">
        <div class="px-4 pt-5 pb-8">
          <h1 class="page-title">设置</h1>

          <div class="mt-5">
            <div class="sec-head"><span>GENERAL</span></div>
            <div class="divide-y divide-white/[0.06]">
              <button v-for="cat in settingsCategories" :key="cat.key" class="kv-row kv-btn">
                <span class="kv-label">{{ cat.label }}</span>
                <span class="kv-value text-zinc-600 font-mono text-xs">{{ catValue(cat.key) }}</span>
                <span class="arrow ml-2">›</span>
              </button>
            </div>
          </div>

          <div class="mt-6">
            <div class="sec-head"><span>EXTENSIONS</span></div>
            <div class="divide-y divide-white/[0.06]">
              <button class="kv-row kv-btn" @click="page = 5">
                <span class="kv-label">插件</span>
                <span class="kv-value text-zinc-500 font-mono text-xs">{{ enabledCount }}/{{ pluginList.length }} ON</span>
                <span class="arrow ml-2">›</span>
              </button>
              <button class="kv-row kv-btn">
                <span class="kv-label">关于</span>
                <span class="kv-value text-zinc-600 font-mono text-xs">v1.1.11</span>
                <span class="arrow ml-2">›</span>
              </button>
            </div>
          </div>

          <div class="mt-6">
            <div class="sec-head text-red-400/70"><span>DANGER</span></div>
            <div class="divide-y divide-white/[0.06]">
              <button class="kv-row kv-btn"><span class="kv-label text-zinc-400">重置设置</span></button>
              <button class="kv-row kv-btn"><span class="kv-label text-red-400">清除全部数据</span></button>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 4 插件 nav tab ========== -->
      <template v-else-if="page === 4">
        <div class="px-4 pt-5 pb-8">
          <h1 class="page-title">Auto Task</h1>
          <p class="mt-1 font-mono text-[10px] text-zinc-600 tracking-wider">PLUGIN NAVTAB · auto-task@1.2.0</p>
          <div class="mt-5">
            <div class="sec-head"><span>SCHEDULED</span><span class="font-mono">3</span></div>
            <div class="divide-y divide-white/[0.06]">
              <div class="data-row">
                <span class="row-idx">01</span>
                <div class="flex-1"><span class="text-[13px]">夜间备份</span><div class="font-mono text-[10px] text-zinc-600 mt-0.5">CRON 0 2 * * *</div></div>
                <span class="state-run font-mono">● ARMED</span>
              </div>
              <div class="data-row">
                <span class="row-idx">02</span>
                <div class="flex-1"><span class="text-[13px]">依赖检查</span><div class="font-mono text-[10px] text-zinc-600 mt-0.5">ON CONNECT</div></div>
                <span class="font-mono text-[10px] text-zinc-600">17 RUNS</span>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 5 插件列表 ========== -->
      <template v-else-if="page === 5">
        <div class="px-4 pt-5 pb-8">
          <div class="flex items-center gap-3">
            <button class="back-btn" @click="page = 3">‹</button>
            <h1 class="page-title">插件</h1>
            <span class="flex-1"></span>
            <button class="square-btn">+ ADD</button>
          </div>

          <div class="mt-5 divide-y divide-white/[0.06] border-t border-white/15">
            <div
              v-for="p in pluginList"
              :key="p.id"
              class="py-3.5 cursor-pointer active:bg-white/[0.03]"
              @click="detail = p"
            >
              <div class="flex items-center gap-2">
                <span class="text-[13px] text-zinc-200">{{ p.name }}</span>
                <span class="font-mono text-[10px] text-zinc-600">v{{ p.version }}</span>
                <span v-if="p.builtin" class="font-mono text-[9px] px-1 py-px border border-white/15 text-zinc-500">BUILTIN</span>
                <span class="flex-1"></span>
                <button
                  class="state-toggle font-mono"
                  :class="p.enabled ? 'st-on' : 'st-off'"
                  @click.stop="p.enabled = !p.enabled"
                >{{ p.enabled ? '[ ON ]' : '[ OFF ]' }}</button>
              </div>
              <div class="mt-1 text-[11px] text-zinc-500 line-clamp-1">{{ p.desc }}</div>
              <div class="mt-1.5 font-mono text-[9px] tracking-wider text-zinc-600">
                {{ p.id.toUpperCase() }} · {{ p.perms }} PERMS · {{ p.size }}
              </div>
            </div>
          </div>

          <!-- 详情：静态全屏页 -->
          <template v-if="detail">
            <div class="fixed inset-0 z-40 bg-[#0c0c0e] overflow-y-auto">
              <div class="flex items-center gap-3 px-4 h-12 border-b border-white/10">
                <button class="back-btn" @click="detail = null">‹</button>
                <span class="font-mono text-xs tracking-wider text-zinc-300">{{ detail.id.toUpperCase() }}</span>
                <span class="flex-1"></span>
                <span class="font-mono text-[10px]" :class="detail.enabled ? 'text-emerald-400' : 'text-zinc-600'">{{ detail.enabled ? 'ACTIVE' : 'IDLE' }}</span>
              </div>
              <div class="px-4 pt-5 pb-10">
                <div class="sec-head"><span>META</span></div>
                <div class="divide-y divide-white/[0.06]">
                  <div class="kv-row"><span class="kv-label">名称</span><span class="kv-value">{{ detail.name }}</span></div>
                  <div class="kv-row"><span class="kv-label">作者</span><span class="kv-value text-zinc-400">{{ detail.author }}</span></div>
                  <div class="kv-row"><span class="kv-label">版本</span><span class="kv-value font-mono">v{{ detail.version }}</span></div>
                  <div class="kv-row"><span class="kv-label">大小</span><span class="kv-value font-mono">{{ detail.size }}</span></div>
                  <div class="kv-row"><span class="kv-label">扩展点</span><span class="kv-value font-mono">{{ detail.chips }}</span></div>
                </div>
                <p class="mt-4 text-xs leading-relaxed text-zinc-500">{{ detail.desc }}</p>
                <div class="mt-5 grid grid-cols-2 gap-2">
                  <button class="h-10 border text-xs font-mono tracking-wider" :class="detail.enabled ? 'border-white/15 text-zinc-300' : 'border-cyan-400/60 text-cyan-400'">
                    {{ detail.enabled ? 'DISABLE' : 'ENABLE' }}
                  </button>
                  <button class="h-10 border text-xs font-mono tracking-wider" :class="detail.builtin ? 'border-white/10 text-zinc-700' : 'border-red-400/40 text-red-400'">
                    {{ detail.builtin ? 'BUILTIN' : 'UNINSTALL' }}
                  </button>
                </div>
              </div>
            </div>
          </template>
        </div>
      </template>
    </div>

    <!-- ==================== 底部导航（发丝线 + 顶部指示线） ==================== -->
    <nav class="vc-nav flex-shrink-0 grid grid-cols-5 border-t border-white/12 pb-2.5">
      <button
        v-for="tab in navTabs"
        :key="tab.key"
        class="relative flex flex-col items-center gap-0.5 pt-2.5 transition-colors"
        :class="isActive(tab.key) ? 'text-cyan-400' : 'text-zinc-600'"
        @click="page = tab.key"
      >
        <span v-if="isActive(tab.key)" class="absolute top-0 left-1/2 -translate-x-1/2 w-6 h-px bg-cyan-400"></span>
        <Icon :d="tab.icon" class="w-5 h-5" />
        <span class="font-mono text-[9px] tracking-wider">{{ tab.label }}</span>
      </button>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, h } from 'vue'
import { I, currentDevice, sessionConfigs, sessions, plugins, settingsCategories, navTabs, type PluginMock } from '../mock'

const Icon = (props: { d: string; class?: string }) =>
  h('svg', { class: props.class ?? 'w-5 h-5', fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24', 'stroke-width': '1.8' }, [
    h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', d: props.d }),
  ])

/** 0 连接 / 1 会话 / 2 工具箱 / 3 设置 / 4 插件 nav tab / 5 插件列表 */
const page = ref(0)
const detail = ref<PluginMock | null>(null)
const pluginList = ref(plugins.map((p) => ({ ...p })))

const runCount = computed(() => sessions.filter((s) => s.status !== 'stopped').length)
const enabledCount = computed(() => pluginList.value.filter((p) => p.enabled).length)

function isActive(key: number) {
  if (key === 3) return page.value === 3 || page.value === 5
  return page.value === key
}
function stateCode(status: string) {
  return status === 'running' ? 'RUN' : status === 'waiting' ? 'WAIT' : 'EXIT'
}
function catValue(key: string) {
  const map: Record<string, string> = {
    connection: 'AUTO-RECONNECT ON',
    notification: 'ON',
    authentication: 'BIOMETRIC',
    appearance: 'DARK · 14PX',
  }
  return map[key] ?? ''
}
</script>

<style scoped>
/* ==================== 驾驶舱语言：发丝线 + 等宽 ==================== */
.page-title {
  font-size: 1.125rem;
  font-weight: 500;
  letter-spacing: 0.02em;
  color: #fafafa;
}
.sec-head {
  display: flex;
  justify-content: space-between;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 9px;
  letter-spacing: 0.25em;
  color: #5c5c66;
  padding-bottom: 0.4rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.15);
}

.kv-row {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 0.625rem 0;
}
.kv-btn { cursor: pointer; transition: background-color 0.12s; }
.kv-btn:active { background: rgba(255, 255, 255, 0.03); }
.kv-label {
  width: 5.5rem;
  flex-shrink: 0;
  font-size: 0.8125rem;
  color: #8b8b96;
  text-align: left;
}
.kv-value { font-size: 0.8125rem; color: #e4e4e7; }

.data-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.75rem 0;
}
.row-idx {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  color: #4a4a54;
  width: 1.25rem;
  flex-shrink: 0;
}

.tool-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  width: 100%;
  padding: 0.875rem 0;
  cursor: pointer;
  transition: background-color 0.12s;
}
.tool-row:active { background: rgba(255, 255, 255, 0.03); }

.arrow { color: #4a4a54; font-size: 1rem; }
.badge-num {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  color: #22d3ee;
  border: 1px solid rgba(34, 211, 238, 0.35);
  padding: 0.05rem 0.4rem;
}

/* ==================== 状态表达 ==================== */
.state-run { font-size: 9px; letter-spacing: 0.12em; color: #34d399; flex-shrink: 0; }
.state-flag {
  font-size: 8px;
  letter-spacing: 0.12em;
  padding: 0.1rem 0.3rem;
  border: 1px solid;
  flex-shrink: 0;
}
.st-running { color: #34d399; border-color: rgba(52, 211, 153, 0.4); }
.st-waiting { color: #fbbf24; border-color: rgba(251, 191, 36, 0.4); }
.st-stopped { color: #6b6b76; border-color: rgba(255, 255, 255, 0.12); }

.env-flag {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 8px;
  letter-spacing: 0.1em;
  padding: 0.1rem 0.3rem;
  flex-shrink: 0;
}
.flag-wsl { color: #a78bfa; background: rgba(167, 139, 250, 0.1); }
.flag-win { color: #22d3ee; background: rgba(34, 211, 238, 0.1); }

.state-toggle { font-size: 10px; letter-spacing: 0.08em; cursor: pointer; flex-shrink: 0; }
.st-on { color: #34d399; }
.st-off { color: #5c5c66; }

/* ==================== 方角按钮 ==================== */
.square-btn {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 9px;
  letter-spacing: 0.14em;
  color: #22d3ee;
  border: 1px solid rgba(34, 211, 238, 0.4);
  padding: 0.3rem 0.6rem;
  flex-shrink: 0;
}
.square-btn:active { background: rgba(34, 211, 238, 0.1); }

.stop-btn, .del-btn {
  width: 1.75rem;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid;
  font-size: 10px;
}
.stop-btn { color: #f87171; border-color: rgba(248, 113, 113, 0.35); }
.stop-btn:active { background: rgba(248, 113, 113, 0.12); }
.del-btn { color: #6b6b76; border-color: rgba(255, 255, 255, 0.15); font-size: 13px; }

.link-btn {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  letter-spacing: 0.12em;
  color: #22d3ee;
  padding-bottom: 0.2rem;
  border-bottom: 1px solid rgba(34, 211, 238, 0.4);
}

.back-btn {
  width: 1.75rem;
  height: 1.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid rgba(255, 255, 255, 0.15);
  color: #d4d4d8;
  font-size: 1rem;
  flex-shrink: 0;
}

.vc-nav { background: #0a0a0c; }
</style>
