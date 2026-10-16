<template>
  <div class="flex flex-col flex-1 min-h-0">
    <!-- 应用运行面：组件由应用提供（或经数据源延迟解析），壳只负责挂载 -->
    <component :is="surface" v-if="surface" :app="app" class="flex-1 min-h-0" />

    <!-- 尚未注册运行面：渲染预留位并写明原因（应用内页面属于应用自己，
         平台不实现；应用注册 surface 后自动顶掉这里） -->
    <ShellPlaceholderSurface v-else-if="app" :app="app" />

    <!-- 应用不存在：给出原因，不留白屏 -->
    <div v-else class="flex flex-1 items-center justify-center px-8 text-center">
      <p class="text-[var(--font-size-base)] text-[var(--mobile-text-secondary)]">
        {{ t('shell.run.notFound') }}
      </p>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 应用运行面容器（应用自持界面的挂载点）
 *
 * 壳在这里与「应用内部」划清边界：壳提供挂载点、生命周期与空态，应用自持全部业务 UI。
 * 渲染优先级：
 *   ① 应用注册的 surface 组件（正路）
 *   ② 数据源延迟解析的运行面（应用 activate 之后才注册面时的兜底）
 *   ③ 预留位 + 空态说明
 *
 * 刻意没有第三条路：早期宿主壳曾按「工具箱页 → 导航 Tab → 终端主视图 → 插件路由」
 * 回退，那等于让壳去认识四种插件形态，与「应用只自持一种运行面」的形态相悖；
 * 找不到面就显式说没有（§5.1.3 fail-visible 形态 ①）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../composables/useShellApps'
import ShellPlaceholderSurface from './ShellPlaceholderSurface.vue'

const props = defineProps<{ appId: string }>()

const { t } = useI18n()
const { getApp, resolveSurface } = useShellApps()

const app = computed(() => getApp(props.appId))
/** 用 appId 而非 app 对象做依赖：应用停止/启动后对象会换，但解析入口不变 */
const surface = computed(() => resolveSurface(props.appId))
</script>
