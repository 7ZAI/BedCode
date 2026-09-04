<script setup lang="ts">
/**
 * PeerHeader — 顶栏：活跃对端 + 连接状态 + 设置入口
 *
 * 对端 chip 整块可点（44px 命中区）→ 切换到设备 tab；连接状态胶囊跟随
 * 纯连接层语义（ws_* 驱动），与旧版 ft-peer-status 同色体系。
 */
defineProps<{
  /** 活跃对端展示名（未连接时由父组件给占位文案） */
  peerName: string
  /** WS 控制面连接态（决定胶囊文案与配色） */
  online: boolean
  onlineLabel: string
  offlineLabel: string
  /** 上传入口 aria/可访问文案 */
  uploadLabel: string
  /** 对端离线时置灰上传（无活跃对端不可选文件） */
  uploadDisabled: boolean
}>()

defineEmits<{
  (e: 'peer-tap'): void
  (e: 'upload'): void
  (e: 'settings'): void
}>()
</script>

<template>
  <div class="flex-shrink-0 flex items-center gap-2 px-4 pt-2.5 pb-2">
    <!-- 对端选择器：整块可点，展开设备列表 -->
    <button class="fv2-peer-chip" @click="$emit('peer-tap')">
      <svg class="w-4 h-4 flex-shrink-0 text-[var(--mobile-text-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
        />
      </svg>
      <span class="fv2-peer-name truncate">{{ peerName }}</span>
      <svg class="fv2-peer-chevron" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <!-- 连接状态胶囊（success tint / 中性底） -->
    <span class="fv2-status-pill" :class="online ? 'fv2-status-pill--online' : 'fv2-status-pill--offline'">
      <span v-if="online" class="fv2-status-dot"></span>
      {{ online ? onlineLabel : offlineLabel }}
    </span>

    <!-- 弹性空隙：把操作按钮推到行尾 -->
    <div class="flex-1 min-w-1"></div>

    <!-- 上传入口：全程常驻（旧版顶栏同语义），对端离线置灰；
         底栏空闲态大 CTA 与开关份共享 uploadFile() 同一路径 -->
    <button
      class="fv2-icon-btn fv2-icon-btn--accent"
      :disabled="uploadDisabled"
      :aria-label="uploadLabel"
      :title="uploadLabel"
      @click="$emit('upload')"
    >
      <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
        />
      </svg>
    </button>

    <!-- 设置入口 -->
    <button class="fv2-icon-btn" @click="$emit('settings')">
      <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
        />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </svg>
    </button>
  </div>
</template>
