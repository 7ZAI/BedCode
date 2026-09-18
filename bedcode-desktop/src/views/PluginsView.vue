<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题+计数，右插件工具栏+刷新（最右） ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <h1 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ $t('desktop.plugin.title') }}
        </h1>
        <span class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]"
          >{{ enabledPlugins.length }}/{{ plugins.length }}
          {{ $t('desktop.plugin.enabledSection') }}</span
        >
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="plugins" />
        <button class="wb-btn-ghost" :disabled="loading" @click="loadPlugins()">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          {{ $t('desktop.plugin.refresh') }}
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-auto p-5">
      <div class="max-w-4xl mx-auto">
        <!-- ==================== 加载态：骨架 ==================== -->
        <div v-if="loading && plugins.length === 0" class="space-y-6">
          <div v-for="i in 2" :key="i">
            <div class="h-3 w-32 rounded animate-pulse bg-[var(--bg-hover)] mb-2"></div>
            <div
              class="h-16 rounded-[10px] animate-pulse bg-[var(--bg-card)] border border-[var(--border)]"
            ></div>
          </div>
        </div>

        <!-- ==================== ENABLED / DISABLED 分区 ==================== -->
        <template v-else>
          <!-- ENABLED 分区（分区头承载「加载插件」入口：该分区渲染时它就是页面第一个标题，
               入口固定在第一个标题右侧，不随分区内容增减跳动） -->
          <section v-if="enabledPlugins.length > 0" class="mb-6">
            <div class="plugin-section-header flex items-center justify-between mb-2">
              <h2 class="wb-section-title">
                {{ $t('desktop.plugin.enabledSection') }} · {{ enabledPlugins.length }}
              </h2>
              <button class="wb-btn-primary" :disabled="installing" @click="showInstallSheet = true">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M12 4v7m0 0l-3-3m3 3l3-3M5 14v3a2 2 0 002 2h10a2 2 0 002-2v-3"
                  />
                </svg>
                {{ $t('desktop.plugin.loadPlugin') }}
              </button>
            </div>
            <div
              class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden"
            >
              <div v-for="plugin in enabledPlugins" :key="plugin.id" class="plugin-row">
                <!-- 主体：图标 + 信息 + 操作 -->
                <div
                  class="flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]"
                  @click="goDetail(plugin.id)"
                >
                  <PluginIcon
                    :icon="plugin.icon"
                    :name="plugin.name"
                    :plugin-id="plugin.id"
                    :extension-path="plugin.extensionPath"
                  />
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span
                        class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
                        >{{ plugin.name }}</span
                      >
                      <span class="wb-mono text-[var(--text-tertiary)] shrink-0"
                        >v{{ plugin.version }}</span
                      >
                      <!-- 降级徽章：实例运行中但启动初始化失败，与完全激活区分（spec §3.6） -->
                      <span
                        v-if="isDegraded(plugin.state)"
                        class="shrink-0 inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400"
                        :title="getDegradedMessage(plugin.state)"
                      >
                        ⚠ {{ $t(getStateKey(plugin.state)) }}
                      </span>
                      <!-- 扩展点 chips -->
                      <span
                        v-for="chip in getContributionChips(plugin)"
                        :key="chip.key"
                        class="shrink-0 inline-flex items-center gap-0.5 px-1 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
                        :title="$t(chip.labelKey, chip.params ?? {})"
                      >
                        {{ chip.emoji }}
                      </span>
                    </div>
                    <!-- 简介折叠区 -->
                    <div class="mt-0.5">
                      <div
                        class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] transition-all duration-200"
                        :class="descExpanded[plugin.id] ? 'whitespace-pre-wrap' : 'truncate'"
                      >
                        {{ plugin.description || $t('desktop.plugin.noDescription') }}
                      </div>
                    </div>
                  </div>
                  <!-- 简介展开 chevron -->
                  <button
                    class="w-5 h-5 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
                    :title="$t('desktop.plugin.openDetail')"
                    @click.stop="descExpanded[plugin.id] = !descExpanded[plugin.id]"
                  >
                    <svg
                      class="w-3.5 h-3.5 transition-transform duration-200"
                      :class="{ 'rotate-90': descExpanded[plugin.id] }"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M9 5l7 7-7 7"
                      />
                    </svg>
                  </button>
                  <!-- 配置入口（仅激活 + 有配置时可点） -->
                  <router-link
                    v-if="hasConfiguration(plugin)"
                    :to="`/plugins/${plugin.id}/config`"
                    class="h-7 px-3 text-xs font-medium rounded-[6px] border border-[var(--border)] text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors shrink-0 flex items-center"
                    @click.stop
                  >
                    {{ $t('desktop.plugin.goConfig') }}
                  </router-link>
                  <span
                    v-else
                    class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center"
                    >{{ $t('desktop.plugin.config') }}</span
                  >
                  <!-- 启停开关（v-bind 改造：CSS 变量驱动样式，状态切换集中在 <style>）；
                       Degraded 实例在运行，开关保持 ON 态（停用语义） -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="plugin-toggle"
                    :style="{
                      '--toggle-on': isRunning(plugin.state) ? 1 : 0,
                      '--toggle-locked': togglingId === plugin.id ? 1 : 0,
                    }"
                    :title="$t('desktop.plugin.disable')"
                    :aria-label="$t('desktop.plugin.disable')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, false)"
                  >
                    <span class="plugin-toggle__knob" />
                  </button>
                  <span
                    v-else
                    class="text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0"
                    >{{ $t('desktop.plugin.alwaysOn') }}</span
                  >
                </div>
              </div>
            </div>
          </section>

          <!-- DISABLED 分区（分区头常显：无已启用插件时它成为页面第一个标题，承接同一入口） -->
          <section>
            <div class="plugin-section-header flex items-center justify-between mb-2">
              <h2 class="wb-section-title">
                {{ $t('desktop.plugin.disabledSection') }} · {{ disabledPlugins.length }}
              </h2>
              <button
                v-if="enabledPlugins.length === 0"
                class="wb-btn-primary"
                :disabled="installing"
                @click="showInstallSheet = true"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M12 4v7m0 0l-3-3m3 3l3-3M5 14v3a2 2 0 002 2h10a2 2 0 002-2v-3"
                  />
                </svg>
                {{ $t('desktop.plugin.loadPlugin') }}
              </button>
            </div>

            <!-- 无任何插件：空态提示（分区头与安装入口保留，不随列表为空消失） -->
            <div v-if="plugins.length === 0" class="py-12 text-center">
              <p class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">
                {{ $t('desktop.plugin.noPlugins') }}
              </p>
              <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] mt-1">
                {{ $t('desktop.plugin.noPluginsHint') }}
              </p>
            </div>

            <!-- 未启用插件列表 -->
            <div
              v-else-if="disabledPlugins.length > 0"
              class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)] overflow-hidden"
            >
              <div v-for="plugin in disabledPlugins" :key="plugin.id" class="plugin-row">
                <div
                  class="flex items-center gap-3 px-4 py-3 cursor-pointer transition-colors hover:bg-[var(--bg-hover)]"
                  @click="goDetail(plugin.id)"
                >
                  <PluginIcon
                    :icon="plugin.icon"
                    :name="plugin.name"
                    :plugin-id="plugin.id"
                    :extension-path="plugin.extensionPath"
                  />
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                      <span
                        class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)] truncate"
                        >{{ plugin.name }}</span
                      >
                      <span class="wb-mono text-[var(--text-tertiary)] shrink-0"
                        >v{{ plugin.version }}</span
                      >
                      <span
                        class="wb-mono text-[calc(11px*var(--ui-scale))] shrink-0"
                        :class="
                          isErrorState(plugin.state)
                            ? 'text-red-600 dark:text-red-400'
                            : 'text-[var(--text-tertiary)]'
                        "
                      >
                        {{
                          isErrorState(plugin.state)
                            ? getErrorMessage(plugin.state)
                            : $t(getStateKey(plugin.state))
                        }}
                      </span>
                    </div>
                    <div class="mt-0.5">
                      <div
                        class="text-[calc(11px*var(--ui-scale))] transition-all duration-200"
                        :class="[
                          descExpanded[plugin.id] ? 'whitespace-pre-wrap' : 'truncate',
                          isErrorState(plugin.state)
                            ? 'text-red-600/70 dark:text-red-400/70'
                            : 'text-[var(--text-secondary)]',
                        ]"
                      >
                        {{
                          isErrorState(plugin.state)
                            ? '⚠ ' + getErrorMessage(plugin.state)
                            : plugin.description || $t('desktop.plugin.noDescription')
                        }}
                      </div>
                    </div>
                  </div>
                  <!-- 简介展开 chevron -->
                  <button
                    class="w-5 h-5 flex items-center justify-center text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
                    :title="$t('desktop.plugin.openDetail')"
                    @click.stop="descExpanded[plugin.id] = !descExpanded[plugin.id]"
                  >
                    <svg
                      class="w-3.5 h-3.5 transition-transform duration-200"
                      :class="{ 'rotate-90': descExpanded[plugin.id] }"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M9 5l7 7-7 7"
                      />
                    </svg>
                  </button>
                  <!-- 配置入口不可用 -->
                  <span
                    class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center"
                    >{{ $t('desktop.plugin.config') }}</span
                  >
                  <!-- 启停开关（v-bind 改造：同一组件，状态由 CSS 变量驱动） -->
                  <button
                    v-if="plugin.pluginType !== 'rust'"
                    class="plugin-toggle"
                    :style="{
                      '--toggle-on': isRunning(plugin.state) ? 1 : 0,
                      '--toggle-locked': togglingId === plugin.id ? 1 : 0,
                    }"
                    :title="$t('desktop.plugin.enabled')"
                    :aria-label="$t('desktop.plugin.enabled')"
                    :disabled="togglingId === plugin.id"
                    @click.stop="handleToggle(plugin.id, true)"
                  >
                    <span class="plugin-toggle__knob" />
                  </button>
                  <span
                    v-else
                    class="h-7 px-3 text-xs text-[var(--text-tertiary)] shrink-0 flex items-center"
                    >{{ $t('desktop.plugin.alwaysOn') }}</span
                  >
                </div>
              </div>
            </div>
          </section>
        </template>
      </div>
    </div>

    <!-- ==================== 启停遮罩弹窗（通用 LoadingOverlay 组件） ==================== -->
    <LoadingOverlay :visible="!!togglingPluginInfo" :message="togglingPluginInfo?.message" />

    <!-- ==================== 加载插件弹窗（从 zip 安装） ==================== -->
    <Modal v-model="showInstallSheet" size="sm" :title="$t('desktop.plugin.loadPluginSheetTitle')">
      <p class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] mb-4 leading-relaxed">
        {{ $t('desktop.plugin.loadPluginHint') }}
      </p>
      <button
        class="w-full flex items-center gap-3 px-4 py-3 rounded-[8px] border border-dashed border-[var(--border-strong)] text-left transition-colors hover:bg-[var(--bg-hover)] disabled:opacity-50"
        :disabled="installing"
        @click="handleInstallFile"
      >
        <svg class="w-4 h-4 text-[var(--color-primary)] shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.75"
            d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
          />
        </svg>
        <span
          class="text-[calc(12px*var(--ui-scale))] font-medium"
          :class="installing ? 'text-[var(--text-tertiary)]' : 'text-[var(--text-primary)]'"
        >
          {{ installing ? $t('desktop.plugin.installing') : $t('desktop.plugin.installFromFile') }}
        </span>
      </button>
      <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-2">
        {{ $t('desktop.plugin.installFromFileDesc') }}
      </p>
      <template #footer>
        <button
          class="w-full py-2 text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)] hover:text-[var(--text-primary)] transition-colors"
          :disabled="installing"
          @click="showInstallSheet = false"
        >
          {{ $t('desktop.plugin.installCancel') }}
        </button>
      </template>
    </Modal>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginsView - 桌面端插件管理页面
 *
 * Warm Workbench 风格：工具栏页头 + ENABLED/DISABLED 分区列表。
 * 卡片主体点击进入详情页，chevron 展开简介折叠区。
 * 启停操作显示全页遮罩弹窗，防反复点击。
 */
import { computed, onActivated, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { usePluginManager } from '@/composables/usePluginManager'
import LoadingOverlay from '@/components/LoadingOverlay.vue'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import PluginIcon from '@/components/PluginIcon.vue'
import Modal from '@/components/Modal.vue'
import { open } from '@tauri-apps/plugin-dialog'
import { pluginInstallFromFile } from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import i18n from '@/locales'
import {
  getContributionChips,
  getStateKey,
  isDegraded,
  getDegradedMessage,
  isErrorState,
  getErrorMessage,
  hasConfiguration,
  isRunning,
} from '@/plugin/contributionKinds'

const router = useRouter()
const { plugins, loading, togglingId, togglingPluginInfo, loadPlugins, togglePlugin } =
  usePluginManager()
const toast = useToast()
const t = i18n.global.t

/** 加载插件弹窗状态 */
const showInstallSheet = ref(false)
const installing = ref(false)

/** 从文件安装：文件选择器选 zip 插件包 → 后端安装 → 刷新列表 */
async function handleInstallFile(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Plugin', extensions: ['zip'] }],
    })
    if (typeof selected !== 'string') return
    installing.value = true
    const pluginId = await pluginInstallFromFile(selected)
    toast.success(t('desktop.plugin.installSuccess', { name: pluginId }))
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('desktop.plugin.installFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    // 无论成功失败都收起弹窗（成功时列表已刷新，失败时 toast 已提示）
    showInstallSheet.value = false
  }
}

/** 简介折叠状态（每个插件独立控制） */
const descExpanded = reactive<Record<string, boolean>>({})

/** 实例运行中（含 Degraded）→ ENABLED 分区；其余 → DISABLED 分区 */
const enabledPlugins = computed(() => plugins.value.filter((p) => isRunning(p.state)))
const disabledPlugins = computed(() => plugins.value.filter((p) => !isRunning(p.state)))

/** 跳转到插件详情页 */
function goDetail(pluginId: string): void {
  router.push({ name: 'plugin-detail', params: { id: pluginId } })
}

/** 处理切换 */
async function handleToggle(id: string, enable: boolean): Promise<void> {
  await togglePlugin(id, enable)
}

onMounted(() => {
  loadPlugins()
})

// KeepAlive 缓存恢复（从详情页返回列表）时 onMounted 不会重新执行，列表会保留
// 进入详情前的旧状态（详情页启停后返回仍显示旧状态）。监听 onActivated 在每次
// 缓存恢复时重新拉取，保证列表与详情页的启用/禁用状态一致。
// 注意：KeepAlive 内首次挂载时 onActivated 也会触发一次（onMounted 已加载过），
// 用标志位跳过，避免双次拉取
let activatedOnce = false
onActivated(() => {
  if (activatedOnce) loadPlugins()
  activatedOnce = true
})
</script>

<style scoped>
.overlay-enter-active,
.overlay-leave-active {
  transition: opacity 0.2s ease;
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}

/* ==================== 未启用分区头（标题 + 加载插件按钮同行） ==================== */
/*
 * wb-section-title 自带 margin-bottom: 8px，在同行布局里会撑高行盒、让标题文字
 * 相对按钮偏心；用更高优先级的后代选择器归零（Tailwind 的 mb-0 工具类在打包
 * 产物中先于该普通规则输出，覆盖不掉）。
 */
.plugin-section-header .wb-section-title {
  margin-bottom: 0;
}

/* ==================== 启停开关 ==================== */
/*
 * v-bind 改造要点：
 * - 状态（on / locked）由父组件通过 :style 注入 CSS 变量（数值：0 或 1）
 * - 视觉表现（背景、边框、圆点位置、opacity）完全由 <style> 集中定义
 * - 切换 on 时只需改变 --toggle-on 数值，圆点 transform 自动位移（calc 参与）
 * - 锁定态（--toggle-locked: 1）通过 calc 乘 0.5 影响 opacity，零额外 DOM
 * - 子组件或子元素可通过 var(--toggle-on) 读取状态，无需 props 透传
 */
.plugin-toggle {
  /* 数值由父组件 :style 注入；这里给默认值保 SSR/初次渲染正确 */
  --toggle-on: 0;
  --toggle-locked: 0;
  --toggle-shift: 19; /* 圆点位移：按钮 40 - 圆点 12 - 左右内边距 3*2 - 边框 1*2 = 19px */

  position: relative;
  width: 2.5rem;
  height: 1.25rem;
  border-radius: 4px;
  border: 1px solid;
  flex-shrink: 0;
  cursor: pointer;
  transition:
    background-color 0.2s,
    border-color 0.2s,
    opacity 0.2s;

  background: var(--color-primary);
  border-color: var(--color-primary);
  /* 锁定 1 时 opacity 0.5，0 时 1；calc(1 - var(--toggle-locked) * 0.5) */
  opacity: calc(1 - var(--toggle-locked) * 0.5);
}

/* 关闭态：覆盖 background / border */
.plugin-toggle[style*='--toggle-on: 0'] {
  background: var(--bg-page);
  border-color: var(--border-strong);
}

.plugin-toggle__knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 2px;
  background: var(--color-primary-contrast);
  transition:
    transform 0.2s,
    background-color 0.2s;
  /* calc 直接用 --toggle-on（数字），不用字符串 */
  transform: translateX(calc(var(--toggle-on) * var(--toggle-shift) * 1px));
}

.plugin-toggle[style*='--toggle-on: 0'] .plugin-toggle__knob {
  background: var(--border-strong);
}
</style>
