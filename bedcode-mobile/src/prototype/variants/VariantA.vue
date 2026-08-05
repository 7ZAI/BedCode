<!--
  PROTOTYPE — 变体 A「秩序」：分组列表语言（一次性原型，勿在生产代码引用）

  设计主张：以现有设置页/插件页为原点向外推广 —— 全部内容收敛进
  圆角分组卡片（divide-y 行 + 彩色图标 chip + uppercase 微标题），
  零散卡片、横幅、发光效果一律取消。层级靠分组与留白，不靠阴影。
-->
<template>
  <div class="va h-full flex flex-col bg-[#0a0a0f] text-zinc-100 select-none">
    <!-- ==================== 页面内容 ==================== -->
    <div class="flex-1 min-h-0 overflow-y-auto">
      <!-- ========== 0 连接 ========== -->
      <template v-if="page === 0">
        <header class="px-5 pt-6 pb-4">
          <div class="flex items-end justify-between">
            <div>
              <h1 class="text-[1.375rem] font-semibold tracking-tight">连接</h1>
              <p class="mt-0.5 text-xs text-zinc-500">已连接主机 · 会话配置随时可用</p>
            </div>
            <button class="text-[13px] font-medium text-cyan-400 pb-1">发现设备</button>
          </div>
        </header>

        <div class="px-4 pb-8 space-y-6">
          <!-- 当前连接 -->
          <section>
            <h2 class="sec-title">当前连接</h2>
            <div class="group-card">
              <div class="row">
                <span class="chip tone-emerald"><Icon :d="I.monitor" /></span>
                <div class="flex-1 min-w-0">
                  <div class="row-title">{{ currentDevice.name }}</div>
                  <div class="row-sub font-mono">{{ currentDevice.address }}</div>
                </div>
                <span class="badge badge-emerald"><span class="dot dot-emerald"></span>已配对</span>
              </div>
              <div class="row">
                <span class="chip tone-zinc"><Icon :d="I.clock" /></span>
                <div class="flex-1"><div class="row-label">连接时长</div></div>
                <span class="row-value font-mono">{{ currentDevice.uptime }}</span>
              </div>
              <button class="row row-btn">
                <span class="chip tone-red"><Icon :d="I.unlink" /></span>
                <span class="flex-1 text-left text-[0.9375rem] font-medium text-red-400">断开连接</span>
              </button>
            </div>
          </section>

          <!-- 会话配置 -->
          <section>
            <div class="flex items-center justify-between px-1 mb-2">
              <h2 class="sec-title !mb-0">会话配置</h2>
              <button class="p-1 text-zinc-500 active:text-cyan-400"><Icon :d="I.refresh" class="w-4 h-4" /></button>
            </div>
            <div class="group-card">
              <div v-for="c in sessionConfigs" :key="c.id" class="row items-start py-3.5">
                <span class="chip" :class="c.env === 'wsl' ? 'tone-violet' : 'tone-cyan'">
                  <Icon :d="I.terminal" />
                </span>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="row-title">{{ c.name }}</span>
                    <span class="env-tag" :class="c.env === 'wsl' ? 'env-wsl' : 'env-win'">{{ c.env === 'wsl' ? 'WSL2' : 'Windows' }}</span>
                  </div>
                  <div class="row-sub font-mono mt-0.5 truncate">{{ c.path }}</div>
                  <button v-if="c.running" class="mt-2 inline-flex items-center gap-1.5 text-xs font-medium text-emerald-400">
                    <span class="dot dot-emerald"></span>{{ c.runningSessionName }} · 查看
                  </button>
                </div>
                <button
                  v-if="!c.running"
                  class="flex-shrink-0 h-8 px-3.5 rounded-lg bg-cyan-400/10 text-cyan-400 text-xs font-semibold active:bg-cyan-400/20"
                >启动</button>
              </div>
            </div>
          </section>
        </div>
      </template>

      <!-- ========== 1 会话 ========== -->
      <template v-else-if="page === 1">
        <header class="px-5 pt-6 pb-4">
          <h1 class="text-[1.375rem] font-semibold tracking-tight">会话</h1>
          <p class="mt-0.5 text-xs text-zinc-500">{{ currentDevice.name }} · {{ sessions.length }} 个会话</p>
        </header>
        <div class="px-4 pb-8">
          <div class="group-card">
            <div v-for="s in sessions" :key="s.id" class="row">
              <span class="chip" :class="statusTone(s.status)">
                <Icon :d="s.status === 'stopped' ? I.terminal : I.play" />
              </span>
              <div class="flex-1 min-w-0">
                <div class="row-title truncate">{{ s.name }}</div>
                <div class="row-sub mt-0.5 flex items-center gap-2">
                  <span>{{ s.type }}</span>
                  <span class="font-mono text-zinc-500">{{ s.elapsed }}</span>
                  <span v-if="s.task" class="text-amber-400/90">{{ s.task }}</span>
                </div>
              </div>
              <span class="badge" :class="statusBadge(s.status)">{{ statusLabel(s.status) }}</span>
              <button
                v-if="s.status !== 'stopped'"
                class="ml-1 w-8 h-8 rounded-lg bg-red-400/10 text-red-400 flex items-center justify-center active:bg-red-400/20"
              >
                <span class="w-2.5 h-2.5 rounded-[2px] bg-current"></span>
              </button>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 2 工具箱 ========== -->
      <template v-else-if="page === 2">
        <header class="px-5 pt-6 pb-4">
          <h1 class="text-[1.375rem] font-semibold tracking-tight">工具箱</h1>
          <p class="mt-0.5 text-xs text-zinc-500">任务与插件工具入口</p>
        </header>
        <div class="px-4 pb-8 space-y-6">
          <section>
            <h2 class="sec-title">工具</h2>
            <div class="group-card">
              <button class="row row-btn">
                <span class="chip tone-cyan"><Icon :d="I.tasks" /></span>
                <div class="flex-1 min-w-0">
                  <div class="row-title">预设任务</div>
                  <div class="row-sub mt-0.5">4 个任务 · 上次运行 22 分钟前</div>
                </div>
                <span class="badge badge-cyan">4</span>
                <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
              </button>
            </div>
          </section>
          <section>
            <h2 class="sec-title">插件视图</h2>
            <div class="group-card">
              <button class="row row-btn">
                <span class="chip tone-violet"><Icon :d="I.folder" /></span>
                <div class="flex-1 min-w-0">
                  <div class="row-title">Git Glance</div>
                  <div class="row-sub mt-0.5">插件提供的工具箱视图</div>
                </div>
                <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
              </button>
            </div>
          </section>
        </div>
      </template>

      <!-- ========== 3 设置 ========== -->
      <template v-else-if="page === 3">
        <header class="px-5 pt-6 pb-4">
          <h1 class="text-[1.375rem] font-semibold tracking-tight">设置</h1>
        </header>
        <div class="px-4 pb-8 space-y-6">
          <section>
            <h2 class="sec-title">通用</h2>
            <div class="group-card">
              <button v-for="cat in settingsCategories" :key="cat.key" class="row row-btn">
                <span class="chip" :class="'tone-' + cat.tone"><Icon :d="cat.icon" /></span>
                <span class="flex-1 text-left row-title">{{ cat.label }}</span>
                <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
              </button>
            </div>
          </section>
          <section>
            <h2 class="sec-title">扩展</h2>
            <div class="group-card">
              <button class="row row-btn" @click="page = 5">
                <span class="chip tone-emerald"><Icon :d="I.puzzle" /></span>
                <span class="flex-1 text-left row-title">插件</span>
                <span class="badge badge-zinc">{{ pluginList.length }}</span>
                <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600" />
              </button>
            </div>
          </section>
          <section>
            <h2 class="sec-title">其他</h2>
            <div class="group-card">
              <button class="row row-btn">
                <span class="chip tone-zinc"><Icon :d="I.info" /></span>
                <span class="flex-1 text-left row-title">关于</span>
                <span class="row-value font-mono">v1.1.11</span>
                <Icon :d="I.chevronR" class="w-4 h-4 text-zinc-600 ml-1" />
              </button>
            </div>
          </section>
          <section>
            <h2 class="sec-title">危险操作</h2>
            <div class="group-card">
              <button class="row row-btn"><span class="flex-1 text-left text-[0.9375rem] text-zinc-300">重置设置</span></button>
              <button class="row row-btn"><span class="flex-1 text-left text-[0.9375rem] text-red-400">清除全部数据</span></button>
            </div>
          </section>
        </div>
      </template>

      <!-- ========== 4 插件 nav tab（模拟插件注入的导航页） ========== -->
      <template v-else-if="page === 4">
        <header class="px-5 pt-6 pb-4">
          <h1 class="text-[1.375rem] font-semibold tracking-tight">插件页</h1>
          <p class="mt-0.5 text-xs text-zinc-500">由插件注入的导航 Tab</p>
        </header>
        <div class="px-4 pb-8">
          <div class="group-card">
            <div class="p-6 text-center">
              <span class="chip tone-emerald mx-auto !w-12 !h-12 !rounded-2xl"><Icon :d="I.puzzle" class="w-6 h-6" /></span>
              <p class="mt-3 text-sm text-zinc-300 font-medium">Auto Task · 运行面板</p>
              <p class="mt-1 text-xs text-zinc-500 leading-relaxed">插件通过 navTab 扩展点注入的整页视图，<br>与内置页面共用同一套分组语言。</p>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 5 插件列表 ========== -->
      <template v-else-if="page === 5">
        <header class="px-5 pt-6 pb-4 flex items-start gap-3">
          <button class="back-btn" @click="page = 3"><Icon :d="I.back" /></button>
          <div class="flex-1">
            <h1 class="text-[1.375rem] font-semibold tracking-tight">插件</h1>
            <p class="mt-0.5 text-xs text-zinc-500">共 {{ plugins.length }} 个 · {{ enabledCount }} 个已启用</p>
          </div>
          <button class="w-9 h-9 rounded-xl bg-cyan-400 text-zinc-950 flex items-center justify-center active:opacity-80">
            <Icon :d="I.plus" class="w-5 h-5" />
          </button>
        </header>
        <div class="px-4 pb-8">
          <div class="group-card">
            <div v-for="p in pluginList" :key="p.id" class="row items-start py-3.5 cursor-pointer active:bg-white/[0.03]" @click="openDetail(p)">
              <span class="letter" :class="letterTone(p.id)">{{ p.name[0] }}</span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="row-title">{{ p.name }}</span>
                  <span class="badge" :class="p.enabled ? 'badge-emerald' : 'badge-zinc'">
                    <span v-if="p.enabled" class="dot dot-emerald"></span>{{ p.enabled ? '运行中' : '已停用' }}
                  </span>
                </div>
                <p class="row-sub mt-0.5 line-clamp-1">{{ p.desc }}</p>
              </div>
              <button
                class="toggle flex-shrink-0"
                :class="p.enabled ? 'toggle-on' : 'toggle-off'"
                @click.stop="p.enabled = !p.enabled"
              ><span class="toggle-knob"></span></button>
            </div>
          </div>
        </div>
      </template>

      <!-- ========== 6 插件详情 ========== -->
      <template v-else-if="page === 6 && detail">
        <header class="px-5 pt-6 pb-2 flex items-center gap-3">
          <button class="back-btn" @click="page = 5"><Icon :d="I.back" /></button>
          <h1 class="text-[1.375rem] font-semibold tracking-tight">插件详情</h1>
        </header>
        <div class="px-4 pb-8">
          <!-- Hero -->
          <div class="mt-2 px-4 py-5 flex items-center gap-4 group-card">
            <span class="letter letter-lg" :class="letterTone(detail.id)">{{ detail.name[0] }}</span>
            <div class="flex-1 min-w-0">
              <h2 class="text-base font-semibold truncate">{{ detail.name }}</h2>
              <p class="text-xs text-zinc-500 mt-0.5">{{ detail.author }} · v{{ detail.version }}</p>
              <span class="badge mt-2" :class="detail.enabled ? 'badge-emerald' : 'badge-zinc'">
                <span v-if="detail.enabled" class="dot dot-emerald"></span>{{ detail.enabled ? '运行中' : '已停用' }}
              </span>
            </div>
          </div>

          <!-- 操作 -->
          <div class="mt-3 grid grid-cols-2 gap-2.5">
            <button class="h-11 rounded-xl text-sm font-semibold" :class="detail.enabled ? 'bg-white/[0.06] text-zinc-300' : 'bg-cyan-400 text-zinc-950'">
              {{ detail.enabled ? '停用' : '启用' }}
            </button>
            <button class="h-11 rounded-xl text-sm font-semibold" :class="detail.builtin ? 'bg-white/[0.04] text-zinc-600' : 'bg-red-400/10 text-red-400'">
              {{ detail.builtin ? '内置' : '卸载' }}
            </button>
          </div>

          <!-- 统计 -->
          <div class="mt-3 group-card grid grid-cols-3 divide-x divide-white/[0.05] text-center">
            <div class="py-3.5">
              <div class="text-sm font-semibold font-mono">{{ detail.chips }}</div>
              <div class="text-[11px] text-zinc-500 mt-0.5">扩展点</div>
            </div>
            <div class="py-3.5">
              <div class="text-sm font-semibold font-mono">{{ detail.perms }}</div>
              <div class="text-[11px] text-zinc-500 mt-0.5">权限</div>
            </div>
            <div class="py-3.5">
              <div class="text-sm font-semibold font-mono">{{ detail.size }}</div>
              <div class="text-[11px] text-zinc-500 mt-0.5">大小</div>
            </div>
          </div>

          <!-- 简介 / 权限 -->
          <section class="mt-6">
            <h2 class="sec-title">简介</h2>
            <div class="group-card p-4">
              <p class="text-sm leading-relaxed text-zinc-400">{{ detail.desc }}</p>
            </div>
          </section>
          <section class="mt-6">
            <h2 class="sec-title">权限</h2>
            <div class="group-card">
              <div v-for="perm in ['session:read', 'terminal:input', 'fs:read']" :key="perm" class="row">
                <span class="chip tone-amber !w-8 !h-8 !rounded-lg"><Icon :d="I.shield" class="w-4 h-4" /></span>
                <span class="flex-1 text-[13px] text-zinc-300">{{ permName(perm) }}</span>
                <span class="font-mono text-[11px] text-zinc-600">{{ perm }}</span>
              </div>
            </div>
          </section>
        </div>
      </template>
    </div>

    <!-- ==================== 底部导航 ==================== -->
    <nav class="va-nav flex-shrink-0 border-t border-white/[0.06] bg-[#0c0c12]/95 backdrop-blur-xl px-1 pb-3 pt-1.5">
      <button
        v-for="tab in navTabs"
        :key="tab.key"
        class="flex-1 flex flex-col items-center gap-1 py-1 rounded-xl transition-colors"
        :class="page === tab.key || (tab.key === 3 && (page === 5 || page === 6)) ? 'text-cyan-400' : 'text-zinc-600'"
        @click="page = tab.key"
      >
        <Icon :d="tab.icon" class="w-[22px] h-[22px]" />
        <span class="text-[10px] font-medium">{{ tab.label }}</span>
        <span v-if="tab.plugin" class="absolute mt-[-3px] ml-7 w-1.5 h-1.5 rounded-full bg-emerald-400"></span>
      </button>
    </nav>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, h } from 'vue'
import { I, currentDevice, sessionConfigs, sessions, plugins, settingsCategories, navTabs, type PluginMock } from '../mock'

/** 图标：stroke 风格 SVG path */
const Icon = (props: { d: string; class?: string }) =>
  h('svg', { class: props.class ?? 'w-5 h-5', fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24', 'stroke-width': '2' }, [
    h('path', { 'stroke-linecap': 'round', 'stroke-linejoin': 'round', d: props.d }),
  ])

/** 页面：0 连接 / 1 会话 / 2 工具箱 / 3 设置 / 4 插件 nav tab / 5 插件列表 / 6 插件详情 */
const page = ref(0)
const detail = ref<PluginMock | null>(null)
/** 本地副本：让开关操作响应式（不直接 mutate 共享 mock） */
const pluginList = ref(plugins.map((p) => ({ ...p })))

const enabledCount = computed(() => pluginList.value.filter((p) => p.enabled).length)

function openDetail(p: PluginMock) {
  detail.value = p
  page.value = 6
}

function statusTone(status: string) {
  return status === 'running' ? 'tone-emerald' : status === 'waiting' ? 'tone-amber' : 'tone-zinc'
}
function statusBadge(status: string) {
  return status === 'running' ? 'badge-emerald' : status === 'waiting' ? 'badge-amber' : 'badge-zinc'
}
function statusLabel(status: string) {
  return status === 'running' ? '运行中' : status === 'waiting' ? '等待输入' : '已停止'
}
function letterTone(id: string) {
  return id === 'auto-task' ? 'letter-cyan' : id === 'git-glance' ? 'letter-violet' : 'letter-amber'
}
function permName(perm: string) {
  return perm === 'session:read' ? '读取会话' : perm === 'terminal:input' ? '终端输入' : '读取文件'
}
</script>

<style scoped>
/* ==================== 分组卡片语言 ==================== */
.sec-title {
  font-size: 0.6875rem;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: #63636e;
  padding-left: 0.375rem;
  margin-bottom: 0.5rem;
}
.group-card {
  background: #131318;
  border: 1px solid rgba(255, 255, 255, 0.055);
  border-radius: 1rem;
  overflow: hidden;
}
.group-card > .row + .row {
  border-top: 1px solid rgba(255, 255, 255, 0.045);
}
.row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.8125rem 1rem;
  min-height: 3.5rem;
}
.row-btn {
  width: 100%;
  cursor: pointer;
  transition: background-color 0.15s ease;
}
.row-btn:active {
  background: rgba(255, 255, 255, 0.03);
}
.row-title {
  font-size: 0.9375rem;
  font-weight: 500;
  color: #f4f4f5;
}
.row-label {
  font-size: 0.9375rem;
  color: #f4f4f5;
}
.row-sub {
  font-size: 0.75rem;
  color: #71717a;
}
.row-value {
  font-size: 0.8125rem;
  color: #a1a1aa;
}

/* ==================== 图标 chip ==================== */
.chip {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.625rem;
  flex-shrink: 0;
}
.tone-cyan { color: #22d3ee; background: rgba(34, 211, 238, 0.12); }
.tone-emerald { color: #34d399; background: rgba(52, 211, 153, 0.12); }
.tone-amber { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
.tone-violet { color: #a78bfa; background: rgba(167, 139, 250, 0.12); }
.tone-red { color: #f87171; background: rgba(248, 113, 113, 0.12); }
.tone-zinc { color: #a1a1aa; background: rgba(255, 255, 255, 0.06); }

/* ==================== 徽章 / 标签 ==================== */
.badge {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  flex-shrink: 0;
  font-size: 11px;
  font-weight: 600;
  padding: 0.2rem 0.55rem;
  border-radius: 999px;
}
.badge-emerald { color: #34d399; background: rgba(52, 211, 153, 0.12); }
.badge-amber { color: #fbbf24; background: rgba(251, 191, 36, 0.12); }
.badge-cyan { color: #22d3ee; background: rgba(34, 211, 238, 0.12); }
.badge-zinc { color: #8e8e99; background: rgba(255, 255, 255, 0.06); }
.dot { width: 6px; height: 6px; border-radius: 999px; }
.dot-emerald { background: #34d399; box-shadow: 0 0 6px rgba(52, 211, 153, 0.8); }

.env-tag {
  font-size: 10px;
  font-weight: 600;
  padding: 0.1rem 0.4rem;
  border-radius: 0.375rem;
  border: 1px solid;
}
.env-wsl { color: #a78bfa; border-color: rgba(167, 139, 250, 0.35); background: rgba(167, 139, 250, 0.08); }
.env-win { color: #22d3ee; border-color: rgba(34, 211, 238, 0.35); background: rgba(34, 211, 238, 0.08); }

/* ==================== 插件字母图标 ==================== */
.letter {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 0.75rem;
  font-size: 1rem;
  font-weight: 700;
  flex-shrink: 0;
}
.letter-lg { width: 3.5rem; height: 3.5rem; font-size: 1.375rem; border-radius: 1rem; }
.letter-cyan { color: #22d3ee; background: linear-gradient(145deg, rgba(34, 211, 238, 0.22), rgba(34, 211, 238, 0.08)); }
.letter-violet { color: #a78bfa; background: linear-gradient(145deg, rgba(167, 139, 250, 0.22), rgba(167, 139, 250, 0.08)); }
.letter-amber { color: #fbbf24; background: linear-gradient(145deg, rgba(251, 191, 36, 0.22), rgba(251, 191, 36, 0.08)); }

/* ==================== 开关 ==================== */
.toggle {
  position: relative;
  width: 44px;
  height: 26px;
  border-radius: 999px;
  transition: background-color 0.2s ease;
}
.toggle-on { background: #22d3ee; }
.toggle-off { background: #3f3f46; }
.toggle-knob {
  position: absolute;
  top: 3px;
  width: 20px;
  height: 20px;
  border-radius: 999px;
  background: #fff;
  transition: left 0.2s cubic-bezier(0.32, 0.72, 0, 1);
}
.toggle-on .toggle-knob { left: 21px; }
.toggle-off .toggle-knob { left: 3px; }

/* ==================== 其他 ==================== */
.back-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.75rem;
  background: #131318;
  border: 1px solid rgba(255, 255, 255, 0.06);
  color: #d4d4d8;
  flex-shrink: 0;
}
.va-nav { position: relative; }
</style>
