<script setup lang="ts">
/**
 * 测试夹具：以 TerminalView 的用法（template ref → defineExpose 公开 API）
 * 驱动 TerminalInputBar，验证「键盘收起时主动 blur 退出编辑态」契约：
 * blurInput() 移除输入光标、isFocused() 反映编辑态。
 */
import { ref } from 'vue'
import TerminalInputBar from '@/components/TerminalInputBar.vue'

const barRef = ref<InstanceType<typeof TerminalInputBar> | null>(null)

function blurInput(): void {
  barRef.value?.blurInput()
}

function isFocused(): boolean {
  return barRef.value?.isFocused() ?? false
}

defineExpose({ blurInput, isFocused })
</script>

<template>
  <TerminalInputBar ref="barRef" />
</template>