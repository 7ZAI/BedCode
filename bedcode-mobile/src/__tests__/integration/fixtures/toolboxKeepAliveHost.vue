<script setup lang="ts">
/**
 * KeepAlive 包装器（测试夹具）：模拟 MobileSwipeContainer 被缓存时
 * ToolboxView 的 deactivate/reactivate 生命周期（用户离开工具箱去插件页再返回）。
 * toggle false → ToolboxView 被 KeepAlive 缓存（deactivated）；
 * toggle true → 重新激活（onActivated 触发）。
 */
import { ref } from 'vue'
import ToolboxView from '@/views/ToolboxView.vue'

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
    <ToolboxView v-if="active" />
    <div v-else class="other-page" />
  </keep-alive>
</template>
