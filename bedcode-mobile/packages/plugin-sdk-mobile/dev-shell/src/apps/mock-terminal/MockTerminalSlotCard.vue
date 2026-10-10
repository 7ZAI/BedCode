<script setup lang="ts">
/**
 * 会话状态磁贴（内置应用贡献给平台首页的快捷卡片）
 *
 * 用来在预览环境里真实跑通 `registerSlot` 这条路径：壳的首页会渲染应用注册的卡片
 * 组件，插件开发者在这里能确认自己的 slot 组件在壳的布局里表现如何（而不是自己
 * 搭一个假首页去验）。
 *
 * 卡片内容由应用自持（壳只传 app 上下文，不解释「N 个会话」是什么）；
 * 应用自带的卡片也不负责打开应用——打开由壳的宫格 / 列表承担。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { sessions } from '../../mock/session'

/** 壳透传的应用上下文（本卡不消费，留着是为了与 slot 契约一致） */
defineProps<{ app?: { id: string; name: string } }>()

const running = ref(0)

function refresh(): void {
  running.value = sessions.value.filter((s) => s.status === 'running').length
}

const timer = setInterval(refresh, 1000)
onMounted(refresh)
onUnmounted(() => clearInterval(timer))

const hint = computed(() => `mock · ${running.value} running`)
</script>

<template>
  <div
    class="w-full text-left rounded-[12px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] p-3.5"
    :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
  >
    <span class="flex items-center gap-1.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      <span
        class="w-1.5 h-1.5 rounded-full flex-shrink-0"
        style="background: var(--mobile-success)"
        aria-hidden="true"
      />
      mock-terminal
    </span>
    <span class="mt-1.5 block text-[var(--font-size-base)] font-semibold text-[var(--mobile-text-primary)]">
      {{ running }} sessions
    </span>
    <span class="mt-1 block text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      {{ hint }}
    </span>
  </div>
</template>