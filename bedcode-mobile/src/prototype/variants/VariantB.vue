<!--
  PROTOTYPE — 变体 B「空御」：Bento 控制中心（一次性原型，勿在生产代码引用）

  设计主张：手机是"遥控器"，首屏先回答"连上了吗、在跑什么"。
  状态优先：大 Hero 磁贴承载连接状态与会话实况，其余内容拆成
  不对称 Bento 磁贴（2fr/1fr 网格），扩散阴影 + 大圆角，单一青色强调。
  导航为悬浮胶囊，而非常规贴边 Tab Bar。
-->
<template>
  <div class="vb relative h-full flex flex-col bg-[#08080c] text-zinc-100 select-none overflow-hidden">
    <!-- 背景微光（固定、不随内容滚动） -->
    <div class="pointer-events-none absolute inset-0">
      <div class="absolute -top-28 -right-20 w-72 h-72 rounded-full bg-cyan-500/[0.07] blur-3xl"></div>
      <div class="absolute bottom-1/3 -left-24 w-64 h-64 rounded-full bg-violet-500/[0.05] blur-3xl"></div>
    </div>

    <div class="relative flex-1 min-h-0 overflow-y-auto">
      <!-- ========== 0 连接 ========== -->
      <template v-if="page === 0">
        <div class="px-5 pt-6">
          <div class="flex items-center justify-between">
            <div>
              <p class="text-[11px] font-semibold tracking-[0.18em] uppercase text-zinc-500">BedCode Remote</p>
              <h1 class="mt-1 text-2xl font-semibold tracking-tight">控制台</h1>
            </div>
            <span class="pill pill-emerald"><span class="pulse-dot"></span>已配对</span>
          </div>

          <!-- Hero 连接磁贴 -->
          <div class="mt-5 tile p-5">
            <div class="flex items-start justify-between">
              <div class="flex items-center gap-3.5">
                <div class="w-12 h-12 rounded-2xl bg-emerald-400/10 border border-emerald-400/20 flex items-center justify-center text-emerald-400">
                  <Icon :d="I.monitor" class="w-6 h-6" />
                </div>
                <div>
                  <div class="text-[1.0625rem] font-semibold tracking-tight">{{ currentDevice.name }}</div>
                  <div class="mt-0.5 font-mono text-xs text-zinc-500">{{ currentDevice.address }}</div>
                </div>
              </div>
              <button class="text-xs font-medium text-red-400/90 bg-red-400/10 rounded-full px-3 py-1.5 active:bg-red-400/20">断开</button>
            </div>
            <div class="mt-4 pt-4 border-t border-white/[0.05] grid grid-cols-3">
              <div>
                <div class="num">{{ currentDevice.uptime }}</div>
                <div class="metric-label">连接时长</div>
              </div>
              <div>
                <div class="num">{{ runCount }}<span class="text-zinc-600">/{{ sessions.length }}</span></div>
                <div class="metric-label">运行会话</div>
              </div>
              <div>
                <div class="num">{{ sessionConfigs.length }}</div>
                <div class="metric-label">会话配置</div>
              </div>
            </div>
          </div>

          <!-- 快速操作：不对称（1 大 + 2 小） -->
          <div class="mt-3 grid grid-cols-3 grid-rows-2 gap-3 h-40">
            <button class="tile tile-hover col-span-2 row-span-2 p-4 flex flex-col items-start justify-between text-left">
              <span class="w-10 h-10 rounded-xl bg-cyan-400/10 text-cyan-400 flex items-center justify-center"><Icon :d="I.qr" /></span>
              <div>
                <div class="text-sm font-semibold">扫码连接</div>
                <div class="text-xs text-zinc-500 mt-0.5">扫描桌面端二维码快速配对</div>
              </div>
            </button>
            <button class="tile tile-hover p-3.5 flex flex-col justify-between text-left">
              <span class="tile-mini-icon"><Icon :d="I.search" class="w-[1.125rem] h-[1.125rem]" /></span>
              <div class="text-[13px] font-medium">手动输入</div>
            </button>
            <button class="tile tile-hover p-3.5 flex flex-col justify-between text-left">
              <span class="tile-mini-icon"><Icon :d="I.wifi" class="w-4.5 h-4.5" /></span>
              <div class="text-[13px] font-medium">发现设备</div>
            </button>
          </div>

          <!-- 会话配置：横滑磁贴 -->
          <h2 class="mt-6 mb-2.5 px-1 tile-section-title">会话配置</h2>
          <div class="flex gap-3 overflow-x-auto pb-2 -mx-5 px-5 snap-x">
            <div v-for="c in sessionConfigs" :key="c.id" class="tile flex-shrink-0 w-56 p-4 snap-start">
              <div class="flex items-center justify-between">
                <span class="env-tag" :class="c.env === 'wsl' ? 'env-wsl' : 'env-win'">{{ c.env === 'wsl' ? 'WSL2' : 'WIN' }}</span>
                <span v-if="c.running" class="text-[11px] font-medium text-emerald-400 flex items-center gap-1"><span class="pulse-dot"></span>运行中</span>
              </div>
              <div class="mt-2.5 text-sm font-semibold truncate">{{ c.name }}</div>
              <div class="mt-1 font-mono text-[11px] text-zinc-600 truncate">{{ c.path }}</div>
              <button
                class="mt-3 w-full h-9 rounded-xl text-xs font-semibold transition-colors"
                :class="c.running ? 'bg-white/[0.06] text-zinc-300' : 'bg-cyan-400 text-zinc-950 active:opacity-90'"
              >{{ c.running ? '进入会话' : '启动会话' }}</button>
            </div>
          </div>

          <!-- 最近连接 chips -->
          <h2 class="mt-5 mb-2.5 px-1 tile-section-title">最近连接</h2>
          <div class="flex gap-2 overflow-x-auto pb-8 -mx-5 px-5">
            <button v-for="h in connectionHistory" :key="h.address" class="chip-pill">
              <span class="w-1.5 h-1.5 rounded-full bg-zinc-600"></span>
              {{ h.name }}
              <span class="text-zinc-600 font-mono text-[10px]">{{ h.last }}</span>
            </button>
          </div>
        </div>
      </template>

      <!-- ========== 1 会话 ========== -->
      <template v-else-if="page === 1">
        <div class="px-5 pt-6 pb-8">
          <div class="flex items-end justify-between">
            <h1 class="text-2xl font-semibold tracking-tight">会话</h1>
            <span class="text-xs text-zinc-500 font-mono">{{ runCount }} running</span>
          </div>

          <!-- 焦点会话：运行中最久的 -->
          <div class="mt-4 tile p-5 border-emerald-400/[0.15]">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2.5">
                <span class="pulse-dot"></span>
                <span class="text-sm font-semibold">{{ sessions[0].name }}</span>
                <span class="pill pill-zinc">{{ sessions[0].type }}</span>
              </div>
              <span class="num text-base">{{ sessions[0].elapsed }}</span>
            </div>
            <!-- 终端实况预览 -->
            <div class="mt-3.5 rounded-xl bg-black/40 border border-white/[0.05] p-3.5 font-mono text-[11px] leading-relaxed">
              <p v-for="(line, i) in terminalPreview" :key="i" :class="i === 0 ? 'text-zinc-300' : 'text-zinc-500'">{{ line }}</p>
            </div>
            <div class="mt-3.5 flex gap-2.5">
              <button class="flex-1 h-10 rounded-xl bg-cyan-400 text-zinc-950 text-sm font-semibold active:opacity-90">进入终端</button>
              <button class="w-10 h-10 rounded-xl bg-red-400/10 text-red-400 flex items-center justify-center active:bg-red-400/20">
                <span class="w-3 h-3 rounded-[3px] bg-current"></span>
              </button>
            </div>
          </div>

          <!-- 其余会话：两列小磁贴 -->
          <div class="mt-3 grid grid-cols-2 gap-3">
            <div v-for="s in sessions.slice(1)" :key="s.id" class="tile tile-hover p-4" :class="{ 'opacity-60': s.status === 'stopped' }">
              <div class="flex items-center justify-between">
                <span class="status-dot" :class="'dot-' + s.status"></span>
                <span class="font-mono text-[11px] text-zinc-500">{{ s.elapsed }}</span>
              </div>
              <div class="mt-2.5 text-[13px] font-semibold truncate">{{ s.name }}</div>
              <div class="mt-0.5 text-[11px] text-zinc-600">{{ s.type }} · {{ statusLabel(s.status) }}</div>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 2 工具箱 ========== -->
      <template v-else-if="page === 2">
        <div class="px-5 pt-6 pb-8">
          <h1 class="text-2xl font-semibold tracking-tight">工具箱</h1>

          <div class="mt-4 grid grid-cols-2 gap-3">
            <!-- 预设任务大磁贴 -->
            <button class="tile tile-hover col-span-2 p-5 flex items-center gap-4 text-left">
              <span class="w-12 h-12 rounded-2xl bg-cyan-400/10 text-cyan-400 flex items-center justify-center"><Icon :d="I.tasks" class="w-6 h-6" /></span>
              <div class="flex-1">
                <div class="text-[15px] font-semibold">预设任务</div>
                <div class="text-xs text-zinc-500 mt-0.5">一键执行常用命令序列</div>
              </div>
              <span class="num text-2xl">4</span>
            </button>
            <!-- 插件视图磁贴 -->
            <button class="tile tile-hover col-span-2 p-5 flex items-center gap-4 text-left">
              <span class="w-12 h-12 rounded-2xl bg-violet-400/10 text-violet-400 flex items-center justify-center"><Icon :d="I.folder" class="w-6 h-6" /></span>
              <div class="flex-1">
                <div class="text-[15px] font-semibold">Git Glance</div>
                <div class="text-xs text-zinc-500 mt-0.5">插件工具箱视图</div>
              </div>
              <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
            </button>
            <!-- 次级入口 -->
            <button class="tile tile-hover p-4 text-left">
              <Icon :d="I.clock" class="w-5 h-5 text-zinc-500" />
              <div class="mt-3 text-[13px] font-medium">任务历史</div>
              <div class="text-[11px] text-zinc-600 mt-0.5">22 分钟前</div>
            </button>
            <button class="tile tile-hover p-4 text-left">
              <Icon :d="I.bolt" class="w-5 h-5 text-zinc-500" />
              <div class="mt-3 text-[13px] font-medium">快速命令</div>
              <div class="text-[11px] text-zinc-600 mt-0.5">发送到活动会话</div>
            </button>
          </div>
        </div>
      </template>

      <!-- ========== 3 设置 ========== -->
      <template v-else-if="page === 3">
        <div class="px-5 pt-6 pb-8">
          <h1 class="text-2xl font-semibold tracking-tight">设置</h1>

          <div class="mt-4 grid grid-cols-2 gap-3">
            <button v-for="cat in settingsCategories" :key="cat.key" class="tile tile-hover p-4 text-left">
              <span class="w-10 h-10 rounded-xl flex items-center justify-center" :class="'tone-' + cat.tone"><Icon :d="cat.icon" /></span>
              <div class="mt-3 text-[13px] font-semibold">{{ cat.label }}</div>
              <div class="text-[11px] text-zinc-600 mt-0.5">{{ catSub(cat.key) }}</div>
            </button>

            <!-- 插件宽磁贴 -->
            <button class="tile tile-hover col-span-2 p-4 flex items-center gap-3.5 text-left" @click="page = 5">
              <span class="w-10 h-10 rounded-xl bg-emerald-400/10 text-emerald-400 flex items-center justify-center"><Icon :d="I.puzzle" /></span>
              <div class="flex-1">
                <div class="text-[13px] font-semibold">插件</div>
                <div class="text-[11px] text-zinc-600 mt-0.5">{{ enabledCount }} 个运行中 · 共 {{ pluginList.length }} 个</div>
              </div>
              <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
            </button>

            <button class="tile tile-hover col-span-2 p-4 flex items-center gap-3.5 text-left">
              <span class="w-10 h-10 rounded-xl bg-white/[0.05] text-zinc-400 flex items-center justify-center"><Icon :d="I.info" /></span>
              <div class="flex-1">
                <div class="text-[13px] font-semibold">关于</div>
              </div>
              <span class="font-mono text-[11px] text-zinc-600">v1.1.11</span>
            </button>
          </div>

          <!-- 危险区：退卡片，纯留白 + 分隔线 -->
          <div class="mt-6 pt-4 border-t border-white/[0.06] space-y-1">
            <button class="w-full py-2.5 text-left text-sm text-zinc-400 active:opacity-70">重置设置</button>
            <button class="w-full py-2.5 text-left text-sm text-red-400/90 active:opacity-70">清除全部数据</button>
          </div>
        </div>
      </template>

      <!-- ========== 4 插件 nav tab ========== -->
      <template v-else-if="page === 4">
        <div class="px-5 pt-6 pb-8">
          <h1 class="text-2xl font-semibold tracking-tight">Auto Task 面板</h1>
          <p class="mt-1 text-xs text-zinc-500">插件 navTab 扩展点注入的页面</p>
          <div class="mt-4 tile p-5">
            <div class="flex items-center gap-2.5">
              <span class="pulse-dot"></span>
              <span class="text-sm font-semibold">夜间备份任务</span>
            </div>
            <p class="mt-2 text-xs text-zinc-500 leading-relaxed">下次运行 02:00 · 每日执行 · 通知已开启。插件页面与内置页共用同一套磁贴语言。</p>
            <div class="mt-4 grid grid-cols-3 gap-2 text-center">
              <div class="rounded-xl bg-white/[0.04] py-2.5"><div class="num">17</div><div class="metric-label">已运行</div></div>
              <div class="rounded-xl bg-white/[0.04] py-2.5"><div class="num">0</div><div class="metric-label">失败</div></div>
              <div class="rounded-xl bg-white/[0.04] py-2.5"><div class="num">3</div><div class="metric-label">任务</div></div>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 5 插件列表 ========== -->
      <template v-else-if="page === 5">
        <div class="px-5 pt-6 pb-8">
          <div class="flex items-center gap-3">
            <button class="back-btn" @click="page = 3"><Icon :d="I.back" /></button>
            <h1 class="flex-1 text-2xl font-semibold tracking-tight">插件</h1>
            <button class="w-10 h-10 rounded-2xl bg-cyan-400 text-zinc-950 flex items-center justify-center active:opacity-90"><Icon :d="I.plus" /></button>
          </div>

          <div class="mt-4 space-y-3">
            <div v-for="p in pluginList" :key="p.id" class="tile tile-hover p-4 cursor-pointer" @click="detail = p">
              <div class="flex items-center gap-3.5">
                <span class="letter" :class="'letter-' + p.id">{{ p.name[0] }}</span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-sm font-semibold truncate">{{ p.name }}</span>
                    <span class="pill" :class="p.enabled ? 'pill-emerald' : 'pill-zinc'">{{ p.enabled ? '运行中' : '停用' }}</span>
                  </div>
                  <p class="mt-0.5 text-xs text-zinc-500 line-clamp-1">{{ p.desc }}</p>
                </div>
                <button
                  class="toggle flex-shrink-0"
                  :class="p.enabled ? 'toggle-on' : 'toggle-off'"
                  @click.stop="p.enabled = !p.enabled"
                ><span class="toggle-knob"></span></button>
              </div>
            </div>
          </div>

          <!-- 详情浮层（静态示意） -->
          <Transition name="sheet">
            <div v-if="detail" class="fixed inset-0 z-40 flex items-end" @click.self="detail = null">
              <div class="absolute inset-0 bg-black/60"></div>
              <div class="relative w-full rounded-t-[1.75rem] bg-[#101016] border-t border-white/10 p-5 pb-10">
                <div class="mx-auto w-9 h-1 rounded-full bg-white/15"></div>
                <div class="mt-4 flex items-center gap-3.5">
                  <span class="letter letter-lg" :class="'letter-' + detail.id">{{ detail.name[0] }}</span>
                  <div class="flex-1">
                    <div class="text-base font-semibold">{{ detail.name }}</div>
                    <div class="text-xs text-zinc-500 mt-0.5">{{ detail.author }} · v{{ detail.version }} · {{ detail.size }}</div>
                  </div>
                </div>
                <p class="mt-3.5 text-sm text-zinc-400 leading-relaxed">{{ detail.desc }}</p>
                <div class="mt-4 flex flex-wrap gap-1.5">
                  <span v-for="n in detail.chips" :key="n" class="pill pill-cyan">扩展点 {{ n }}</span>
                  <span class="pill pill-zinc">{{ detail.perms }} 项权限</span>
                </div>
                <div class="mt-5 grid grid-cols-2 gap-2.5">
                  <button class="h-11 rounded-2xl text-sm font-semibold" :class="detail.enabled ? 'bg-white/[0.07] text-zinc-200' : 'bg-cyan-400 text-zinc-950'">{{ detail.enabled ? '停用' : '启用' }}</button>
                  <button class="h-11 rounded-2xl text-sm font-semibold" :class="detail.builtin ? 'bg-white/[0.04] text-zinc-600' : 'bg-red-400/10 text-red-400'">{{ detail.builtin ? '内置插件' : '卸载' }}</button>
                </div>
              </div>
            </div>
          </Transition>
        </div>
      </template>
    </div>

    <!-- ==================== 悬浮胶囊导航 ==================== -->
    <nav class="relative flex-shrink-0 px-4 pb-4 pt-2">
      <div class="mx-auto max-w-sm rounded-full bg-[#141419]/95 backdrop-blur-xl border border-white/[0.08] shadow-[0_12px_40px_rgba(0,0,0,0.55)] px-2 py-2 flex justify-between">
        <button
          v-for="tab in navTabs"
          :key="tab.key"
          class="relative flex flex-col items-center justify-center w-12 h-11 rounded-full transition-all duration-300"
          :class="isActive(tab.key) ? 'bg-cyan-400/15 text-cyan-400' : 'text-zinc-600'"
          @click="page = tab.key"
        >
          <Icon :d="tab.icon" class="w-[21px] h-[21px]" />
          <span v-if="tab.plugin" class="absolute top-1 right-1.5 w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
        </button>
      </div>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, h } from 'vue'
import {
  I, currentDevice, sessionConfigs, sessions, plugins, settingsCategories,
  navTabs, connectionHistory, terminalPreview, type PluginMock,
} from '../mock'

const Icon = (props: { d: string; class?: string }) =>
  h('svg', { class: props.class ?? 'w-5 h-5', fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24', 'stroke-width': '2' }, [
    h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', d: props.d }),
  ])

const page = ref(0)
const detail = ref<PluginMock | null>(null)
/** 本地副本：原型内开关操作不污染共享 mock */
const pluginList = ref(plugins.map((p) => ({ ...p })))

const runCount = computed(() => sessions.filter((s) => s.status !== 'stopped').length)
const enabledCount = computed(() => pluginList.value.filter((p) => p.enabled).length)

function isActive(key: number) {
  if (key === 3) return page.value === 3 || page.value === 5
  return page.value === key
}
function statusLabel(status: string) {
  return status === 'running' ? '运行中' : status === 'waiting' ? '等待输入' : '已停止'
}
function catSub(key: string) {
  const map: Record<string, string> = {
    connection: '自动重连 · 端口',
    notification: '任务完成提醒',
    authentication: '生物识别 · 配对',
    appearance: '主题 · 字号',
  }
  return map[key] ?? ''
}
</script>

<style scoped>
/* ==================== 磁贴语言 ==================== */
.tile {
  background: rgba(255, 255, 255, 0.035);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 1.375rem;
  box-shadow: 0 18px 40px -18px rgba(0, 0, 0, 0.55);
}
.tile-hover { transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.2s; }
.tile-hover:active { transform: scale(0.98); background: rgba(255, 255, 255, 0.05); }

.tile-section-title {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.12em;
  text-transform: uppercase;
  color: #5b5b66;
}
.tile-mini-icon {
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.75rem;
  background: rgba(255, 255, 255, 0.05);
  color: #a1a1aa;
  display: flex;
  align-items: center;
  justify-content: center;
}

.num {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 0.9375rem;
  font-weight: 600;
  color: #f4f4f5;
}
.metric-label {
  font-size: 10px;
  color: #6b6b76;
  margin-top: 2px;
}

/* ==================== Pill / dot ==================== */
.pill {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  font-size: 11px;
  font-weight: 600;
  padding: 0.25rem 0.625rem;
  border-radius: 999px;
  flex-shrink: 0;
}
.pill-emerald { color: #34d399; background: rgba(52, 211, 153, 0.12); }
.pill-cyan { color: #22d3ee; background: rgba(34, 211, 238, 0.1); }
.pill-zinc { color: #8e8e99; background: rgba(255, 255, 255, 0.06); }

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #34d399;
  box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.5);
  animation: vb-pulse 2s ease-out infinite;
  flex-shrink: 0;
}
@keyframes vb-pulse {
  0% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.45); }
  70% { box-shadow: 0 0 0 7px rgba(52, 211, 153, 0); }
  100% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0); }
}

.status-dot { width: 8px; height: 8px; border-radius: 999px; }
.dot-running { background: #34d399; }
.dot-waiting { background: #fbbf24; }
.dot-stopped { background: #52525b; }

.chip-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  flex-shrink: 0;
  font-size: 12px;
  font-weight: 500;
  color: #d4d4d8;
  padding: 0.5rem 0.875rem;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.07);
}

.env-tag {
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.06em;
  padding: 0.15rem 0.45rem;
  border-radius: 0.4rem;
}
.env-wsl { color: #a78bfa; background: rgba(167, 139, 250, 0.12); }
.env-win { color: #22d3ee; background: rgba(34, 211, 238, 0.12); }

.tone-cyan { color: #22d3ee; background: rgba(34, 211, 238, 0.12); }
.tone-amber { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
.tone-emerald { color: #34d399; background: rgba(52, 211, 153, 0.12); }
.tone-violet { color: #a78bfa; background: rgba(167, 139, 250, 0.12); }

/* ==================== 插件字母图标 ==================== */
.letter {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.625rem;
  height: 2.625rem;
  border-radius: 1rem;
  font-weight: 700;
  font-size: 1.0625rem;
  flex-shrink: 0;
}
.letter-lg { width: 3.25rem; height: 3.25rem; font-size: 1.25rem; }
.letter-auto-task { color: #22d3ee; background: linear-gradient(150deg, rgba(34, 211, 238, 0.24), rgba(34, 211, 238, 0.06)); border: 1px solid rgba(34, 211, 238, 0.2); }
.letter-git-glance { color: #a78bfa; background: linear-gradient(150deg, rgba(167, 139, 250, 0.24), rgba(167, 139, 250, 0.06)); border: 1px solid rgba(167, 139, 250, 0.2); }
.letter-term-themes { color: #fbbf24; background: linear-gradient(150deg, rgba(251, 191, 36, 0.24), rgba(251, 191, 36, 0.06)); border: 1px solid rgba(251, 191, 36, 0.2); }

/* ==================== 开关 ==================== */
.toggle { position: relative; width: 46px; height: 28px; border-radius: 999px; transition: background-color 0.2s; }
.toggle-on { background: #22d3ee; }
.toggle-off { background: #33333c; }
.toggle-knob {
  position: absolute;
  top: 3px;
  width: 22px;
  height: 22px;
  border-radius: 999px;
  background: #fff;
  transition: left 0.2s cubic-bezier(0.32, 0.72, 0, 1);
}
.toggle-on .toggle-knob { left: 21px; }
.toggle-off .toggle-knob { left: 3px; }

.back-btn {
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 1rem;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.07);
  display: flex;
  align-items: center;
  justify-content: center;
  color: #d4d4d8;
  flex-shrink: 0;
}

/* 详情浮层过渡 */
.sheet-enter-active, .sheet-leave-active { transition: opacity 0.25s ease; }
.sheet-enter-active > div:last-child, .sheet-leave-active > div:last-child {
  transition: transform 0.3s cubic-bezier(0.32, 0.72, 0, 1);
}
.sheet-enter-from, .sheet-leave-to { opacity: 0; }
.sheet-enter-from > div:last-child, .sheet-leave-to > div:last-child { transform: translateY(100%); }

/* 隐藏横滑滚动条 */
.vb ::-webkit-scrollbar { display: none; }
</style>
