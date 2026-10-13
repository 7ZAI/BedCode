<script setup lang="ts">
/**
 * KeepAlive 深层子组件包装器（测试夹具）：忠实复刻真实结构
 * KeepAlive > MobileSwipeContainer(中间组件) > ToolboxView(深层子组件)。
 * 真实场景里 ToolboxView 不是 KeepAlive 直接子级，而是 MobileSwipeContainer
 * v-for 里 <component :is> 渲染的深层子组件——验证 reactivate 时深层子组件
 * 是否重新渲染、registry 变更是否到达它。
 */
import { ref, defineComponent, h } from 'vue'
import ToolboxView from '@/views/ToolboxView.vue'

// 中间组件：渲染 ToolboxView 为其子（模拟 MobileSwipeContainer 包裹 ToolboxView）
const Intermediate = defineComponent({
  name: 'IntermediateShell',
  setup() {
    return () => h(ToolboxView)
  },
})

const active = ref(true)
function deactivate(): void {
  active.value = false
}
function reactivate(): void {
  active.value = true
}
defineExpose({ deactivate, reactivate })
</script>

<template>
  <keep-alive>
    <Intermediate v-if="active" />
    <div v-else class="other-page" />
  </keep-alive>
</template>
