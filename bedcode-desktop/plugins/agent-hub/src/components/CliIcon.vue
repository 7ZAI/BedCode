<script setup lang="ts">
/**
 * Agent CLI 官方图标（概览卡片）
 *
 * 图元数据单一真源在 src/icons.ts（CliIcon 与会话日志 AgentIcon 共用），
 * 本组件只负责芯片容器与品牌色 class；品牌标为固定商标色，不随主题。
 */
import { computed } from 'vue'
import type { CliId } from '../types'
import { CLI_ICONS } from '../icons'

const props = defineProps<{ cliId: CliId }>()

const icon = computed(() => CLI_ICONS[props.cliId])
</script>

<template>
  <span class="ah-cli-ic" :class="`brand-${cliId}`">
    <svg :viewBox="icon.viewBox" aria-hidden="true" focusable="false">
      <path
        v-for="(p, i) in icon.paths"
        :key="i"
        :d="p.d"
        :fill="icon.color"
        :fill-rule="p.rule ?? 'nonzero'"
      />
    </svg>
  </span>
</template>
