<template>
  <nav
    class="backdrop-blur-xl flex-shrink-0"
    :style="{ background: 'var(--mobile-nav-bg)', borderTop: '1px solid var(--mobile-group-border)', paddingBottom: `${bottomInset}px` }"
    :aria-label="t('app.nav.label')"
  >
    <div class="flex justify-around relative">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        class="flex flex-col items-center gap-1 px-4 pt-2.5 pb-2 rounded-xl transition-colors relative min-h-[var(--mobile-nav-item-height)]"
        :class="tab.id === active ? 'text-[var(--mobile-nav-active)]' : ''"
        :style="tab.id === active ? {} : { color: 'var(--mobile-nav-inactive)' }"
        :aria-current="tab.id === active ? 'page' : undefined"
        @click="emit('select', tab.id)"
      >
        <!-- 激活态顶部指示条：与图标严格等宽同轴（left/right 锚定 + margin auto，不依赖 transform 精度） -->
        <span
          v-if="tab.id === active"
          class="absolute top-0 left-0 right-0 mx-auto w-6 h-[2px] rounded-full"
          :style="{ background: 'var(--mobile-nav-active)' }"
          aria-hidden="true"
        />
        <span class="relative flex-shrink-0">
          <!-- 图标按页签分支而非 v-html：路径数据留在模板里可被静态检查，且不引入 innerHTML 注入面 -->
          <svg
            v-if="tab.id === 'connection'"
            class="w-6 h-6"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <!-- 顶部信号柱 -->
            <rect x="9" y="2" width="6" height="4" rx="1" />
            <!-- 中置设备托盘 -->
            <path d="M5 17h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2z" />
            <!-- 底部插脚（旧版 M10 19v-3.96 3.15 为损坏路径数据且偏左，已修复并居中） -->
            <path d="M12 17v4" />
            <!-- 底部基座 -->
            <path d="M8.5 22.5h7" />
          </svg>
          <svg
            v-else-if="tab.id === 'sessions'"
            class="w-6 h-6"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2z" />
          </svg>
          <svg
            v-else-if="tab.id === 'toolbox'"
            class="w-6 h-6"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <!-- 箱体（y=9.5 使整体垂直几何中心落 12，与会话/设置同一纵横标尺） -->
            <rect x="3" y="9.5" width="18" height="10" rx="2" />
            <!-- 居中提手 -->
            <path d="M9 9.5v-2a3 3 0 0 1 6 0v2" />
          </svg>
          <svg
            v-else
            class="w-6 h-6"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </span>
        <span
          class="text-[var(--font-size-sm)]"
          :class="tab.id === active ? 'font-semibold' : 'font-medium'"
        >
          {{ t(tab.labelKey) }}
        </span>
      </button>
    </div>
  </nav>
</template>

<script setup lang="ts">
/**
 * 应用底部导航（票 2026-10-10：全量 UI 下沉 —— 自旧宿主逐字复刻）
 *
 * 视觉与交互真源：`e92cc40a3^:bedcode-mobile/src/components/MobileNav.vue`
 * （旧宿主底部导航，票 2026-10-09 阶段 B 随旧宿主 UI 退役删除）。
 * 差异仅两处，均为机制性：
 * 1. 激活态由宿主路由 / 滑动容器改为 app 内页签状态机（props + emit，不引 vue-router）
 * 2. 插件页签绿点随旧宿主 PluginView 退役一并删除——应用清单由宿主壳承担，
 *    不再由导航条承载插件入口
 *
 * 图标几何注释全部保留：连接/工具箱图标是对称构图（几何中心与顶部重心均落 x=12），
 * 指示条才能严格压在图标头顶正中——换形前先读注释。
 */
import { computed, inject, type Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import type { AppTab, AppTabDef } from '../useAppTabs'

defineProps<{
  tabs: AppTabDef[]
  active: AppTab
}>()

const emit = defineEmits<{ select: [id: AppTab] }>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string): string => context.i18n.t(key)

// 安全区由宿主 App.vue 注入（插件运行面在宿主组件树内，inject 沿树可见）
const safeArea = inject<Ref<{ top: number; bottom: number; navigationBar?: number }> | undefined>(
  'safeArea',
  undefined,
)

// Android WebView 不支持 env(safe-area-inset-*)，只能取 JS 值
const bottomInset = computed(() => {
  const value = safeArea?.value
  return value?.navigationBar ?? value?.bottom ?? 0
})
</script>