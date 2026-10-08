<template>
  <div class="flex flex-col flex-1 min-h-0">
    <!-- 应用运行面：组件由应用提供（或经数据源延迟解析），壳只负责挂载 -->
    <component :is="surface" v-if="surface" :app="app" class="flex-1 min-h-0" />

    <!-- 尚未注册运行面：渲染预留位（终端 / AI Chatbox / 文件传输等应用内页面
         都属于应用自己，平台不实现，接入后由应用注册 surface 顶掉这里） -->
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
 * 应用运行面容器（wasm-app 预留挂载点）
 *
 * 壳在这里与「应用内部」划清边界：壳提供挂载点、生命周期与空态，应用自持全部业务 UI。
 * 渲染优先级：
 *   ① 应用注册的 surface 组件（wasm-app 接入后的正路）
 *   ② 数据源延迟解析的运行面（当前插件形态：工具箱页 / 导航 Tab / 插件路由）
 *   ③ 预留位 + 空态说明（尚未接入时）
 *
 * 原型的终端、AI Chatbox、文件传输界面属于 ① 的范畴，不在本仓库实现。
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
