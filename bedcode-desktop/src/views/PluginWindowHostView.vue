<template>
  <div class="h-screen w-full overflow-hidden bg-[var(--bg-page)]">
    <PluginViewHost
      v-if="viewComponent"
      :key="targetKey"
      :plugin-id="target.pluginId"
      :view-id="target.viewId"
    />
    <div v-else-if="ready" class="flex h-full items-center justify-center px-6">
      <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-secondary)]">
        {{ $t('desktop.plugin.windowTargetMissing', target) }}
      </p>
    </div>
    <div v-else class="flex h-full items-center justify-center">
      <p class="text-[calc(13px*var(--ui-scale))] text-[var(--text-tertiary)]">
        {{ $t('desktop.plugin.windowLoading') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 插件窗口宿主（通用）— 独立 WebviewWindow 的内容一律由插件视图贡献
 *
 * 2026-09-24：原 `TerminalWindowHostView.vue` 只认死一个会话插件视图，本组件把它
 * 通用化——目标插件 / 视图取路由参数（`pluginId` / `viewId`，query 同名参数亦可覆盖），
 * 缺省回落到会话插件的终端窗口视图，故 `/terminal-window/:id` 深链行为不变。
 *
 * 宿主不再自带窗口内容，本组件只做三件事：
 * 1. 解析目标（插件 + 视图），并在目标变化时重解析；
 * 2. **等到目标插件就绪再挂载视图宿主**。这一步是修复终端窗口白屏的关键：
 *    插件由 `main.ts` 非阻塞加载，窗口路由却在首帧就挂载——PluginViewHost
 *    那时既拿不到视图组件（会永久显示「插件视图未找到」），也没机会在 setup
 *    里 provide `pluginContext`（provide 只在 setup 生效，晚注册救不回来）；
 * 3. 注入宿主能力桥（终端侧的宿主存储面 / 命令面原语，见 `provideTerminalHostCapabilities`）。
 *
 * 「等就绪」的两段式：`ensureLoaded()` 等启动加载结束（幂等，不重复 import/activate），
 * 之后仍未 activating 的插件走一次懒激活（未随系统自动激活的场景）。
 * 就绪后仍查不到目标视图 → 显性报错（真源换地方就要 fail-visible），
 * 且因 registry 视图索引是响应式的，插件补注册 / 停用都能自动跟上。
 */
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import PluginViewHost from '@/plugin/components/PluginViewHost.vue'
import { pluginLoader } from '@/plugin/loader'
import { getPluginRegistry } from '@/plugin/registry'
import { provideTerminalHostCapabilities } from '@/plugin/terminal-host-capabilities'
import { logger } from '@/utils/frontendLogger'

/** 会话插件常量（与 plugins/terminal-session/plugin.json 一致；同 src/plugin/context.ts） */
const DEFAULT_WINDOW_PLUGIN_ID = 'com.bedcode.terminal-session'
/** 终端窗口视图 id（会话插件 `kind: 'page'` 贡献面，不进侧边栏菜单） */
const DEFAULT_WINDOW_VIEW_ID = 'session.terminal-window'

const route = useRoute()
const registry = getPluginRegistry()

/** 路由参数可能重复（repeatable params 是数组），只取第一个字符串值 */
function firstString(...values: unknown[]): string {
  for (const v of values) {
    if (typeof v === 'string' && v) return v
    if (Array.isArray(v) && typeof v[0] === 'string' && v[0]) return v[0]
  }
  return ''
}

/** 当前窗口要渲染的插件视图目标 */
const target = computed(() => ({
  pluginId: firstString(route.params.pluginId, route.query.pluginId) || DEFAULT_WINDOW_PLUGIN_ID,
  viewId: firstString(route.params.viewId, route.query.viewId) || DEFAULT_WINDOW_VIEW_ID,
}))

const targetKey = computed(() => `${target.value.pluginId}:${target.value.viewId}`)

/**
 * 目标视图组件（响应式：晚注册与插件停用都能跟上）
 *
 * key 变化会重建 PluginViewHost 实例 —— 同一路由记录切换目标时组件实例会被复用、
 * setup 不重跑，只有重建才能让新插件的 `pluginContext` 在 setup 里完成 provide。
 */
const viewComponent = computed(() =>
  registry.getViewComponent(target.value.pluginId, target.value.viewId),
)

/** 目标插件是否已等到「加载 / 激活」结束（区分「还在加载」与「加载完仍缺失」） */
const ready = ref(false)

async function waitUntilReady(pluginId: string, viewId: string): Promise<void> {
  ready.value = false
  await pluginLoader.ensureLoaded()
  if (!pluginLoader.getActivePlugin(pluginId)) {
    try {
      await pluginLoader.activate(pluginId)
    } catch (e) {
      logger.error(`[PluginWindowHost] Activate failed, plugin_id = ${pluginId}:`, e)
    }
  }
  if (!registry.getViewComponent(pluginId, viewId)) {
    logger.warn(
      `[PluginWindowHost] View still missing after readiness, plugin_id = ${pluginId}, view_id = ${viewId}`,
    )
  }
  ready.value = true
}

watch(
  target,
  (next) => {
    void waitUntilReady(next.pluginId, next.viewId)
  },
  { immediate: true },
)

// 宿主能力桥注入（终端侧的宿主原语；未被渲染插件消费时无副作用）
provideTerminalHostCapabilities()
</script>
