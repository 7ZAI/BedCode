<script setup lang="ts">
/**
 * TabBar — 主分段控件（传输 / 浏览 / 设备）
 *
 * 分段控件：整块中性底 + 激活段白/米白纸面（与桌面端 .ft-tab--active 的 bg-card 同原则，
 * 深色主题为纸白 accent 底 + 墨色文字）。角标提示未读数
 * （传输中数量 / 待处理批请求 / 附近设备数），tabular-nums 防宽度跳动。
 * 左右滑切换由父级 useSwipeTabs 挂在内容区上，本组件只做点选。
 */
import type { MainTab } from '../types'

defineProps<{
  modelValue: MainTab
  transfers: string
  browse: string
  devices: string
  /** 传输中数量（>0 时在传输 tab 显示角标） */
  activeCount: number
  /** 附近设备数量 */
  deviceCount: number
}>()

defineEmits<{
  (e: 'update:modelValue', value: MainTab): void
}>()

/** tab 定义：图标 path（heroicons outline）+ i18n key 由父组件翻译后传入 */
const TABS: Array<{ key: MainTab; icon: string }> = [
  {
    key: 'transfers',
    icon: 'M13 10V3L4 14h7v7l9-11h-7z',
  },
  {
    key: 'browse',
    icon: 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z',
  },
  {
    key: 'devices',
    icon: 'M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.125-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z',
  },
]
</script>

<template>
  <div class="flex-shrink-0 px-4 pb-2">
    <div class="fv2-tabs">
      <button
        v-for="tab in TABS"
        :key="tab.key"
        class="fv2-tab"
        :class="{ 'fv2-tab--active': modelValue === tab.key }"
        @click="$emit('update:modelValue', tab.key)"
      >
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" :d="tab.icon" />
        </svg>
        <span>{{
          tab.key === 'transfers' ? transfers : tab.key === 'browse' ? browse : devices
        }}</span>
        <!-- 传输中角标：仅 >0 时渲染，避免空占位 -->
        <span v-if="tab.key === 'transfers' && activeCount > 0" class="fv2-tab-badge">
          {{ activeCount }}
        </span>
        <!-- 附近设备角标：仅 >0 时渲染 -->
        <span v-else-if="tab.key === 'devices' && deviceCount > 0" class="fv2-tab-badge">
          {{ deviceCount }}
        </span>
      </button>
    </div>
  </div>
</template>
