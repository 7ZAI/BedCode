<script setup lang="ts">
/**
 * PeerDevicesSheet — 附近设备 bottom sheet (Mobile)
 *
 * 列出全部发现的 BedCode 节点：已连接 / 连接中 / 未连接三态可区分，
 * 拨号失败（拒绝/不可达）以行内错误文案呈现；已连接设备可断开、
 * 可设为当前活跃对端；无传输能力节点可见但不可连接。
 * 头部提供「探索发现」重新扫描同网节点（父组件维护 scanning 态）。
 * 纯展示组件：状态由 usePeerDevices 派生的 rows 传入，操作以事件上抛；
 * 视觉语言复用 TaskQueueSheet（Teleport + ft-sheet 过渡 + safe area 底距）。
 */
import type { DeviceRow } from '../composables/deviceState'

type Translate = (key: string, params?: Record<string, any>) => string

const props = defineProps<{
  open: boolean
  rows: DeviceRow[]
  /** 探索发现进行中（扫描按钮转 spinner、防重复点击） */
  scanning?: boolean
  t: Translate
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'connect', nodeId: string): void
  (e: 'disconnect', nodeId: string): void
  (e: 'set-active', nodeId: string): void
  /** 探索发现：重新扫描同网节点（父组件调 query-peer 并维护 scanning） */
  (e: 'scan'): void
}>()

const t = props.t

/** 短指纹（前 8 位）展示 */
function shortFingerprint(nodeId: string): string {
  return nodeId.slice(0, 8)
}

/** 状态点样式：绿=已连接、琥珀呼吸=握手中、灰=未连接/失败 */
function dotClass(row: DeviceRow): string {
  if (row.status === 'connected') return 'ft-dev-dot--online'
  if (row.status === 'connecting') return 'ft-dev-dot--connecting'
  // 不可达终态降为离线灰；在线未连接用降饱和绿与已连接区分（与桌面同构，评审 P2）
  if (row.dialError === 'unreachable') return 'ft-dev-dot--offline'
  return 'ft-dev-dot--reachable'
}

/** 行内元信息文案 key（按三态；addr 仅在线/已连接态追加） */
function metaText(row: DeviceRow): string {
  switch (row.status) {
    case 'connected':
      return row.addr
        ? `${t('transfer.devices.connected')} · ${row.addr}`
        : t('transfer.devices.connected')
    case 'connecting':
      return t('transfer.devices.connecting')
    default:
      return row.addr
        ? `${t('transfer.devices.online')} · ${row.addr}`
        : t('transfer.devices.online')
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="ft-sheet">
      <div v-if="open" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui">
        <!-- Backdrop：点击空白处关闭 -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="emit('close')"></div>

        <!-- Panel -->
        <div class="ft-dev-panel relative w-full flex flex-col bg-[var(--mobile-bg-card)] border-t border-[var(--mobile-border)] rounded-t-2xl shadow-xl">
          <!-- 抓把 -->
          <div class="flex-shrink-0 flex justify-center pt-2.5 pb-1">
            <div class="w-10 h-1 rounded-full bg-[var(--mobile-border-hover)]"></div>
          </div>

          <!-- 标题行：标题 + 副标题 + 行尾「探索发现」重扫按钮（44px 触控目标） -->
          <div class="flex-shrink-0 flex items-center gap-2 px-4 py-2">
            <h3 class="ft-dev-title text-[var(--mobile-text-primary)]">
              {{ t('transfer.devices.title') }}
            </h3>
            <span class="ft-dev-subtitle flex-1 min-w-0 text-[var(--mobile-text-muted)] truncate">
              {{ t('transfer.devices.subtitle') }}
            </span>
            <button
              class="ft-dev-scan-btn flex-shrink-0"
              :disabled="props.scanning"
              @click="emit('scan')"
            >
              <!-- 扫描中：旋转圆环占位（GPU 合成 transform）；否则静态放大镜 -->
              <svg
                v-if="props.scanning"
                class="ft-dev-scan-spin"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
              <svg v-else class="ft-dev-scan-ico" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                />
              </svg>
              {{ props.scanning ? t('transfer.devices.scanning') : t('transfer.devices.scan') }}
            </button>
          </div>

          <!-- 空态：发现缓存无节点 -->
          <div
            v-if="rows.length === 0"
            class="flex-1 min-h-0 overflow-y-auto px-4 py-10 text-center"
          >
            <p class="ft-dev-empty">{{ t('transfer.devices.empty') }}</p>
          </div>

          <!-- 设备行列表 -->
          <div
            v-else
            class="flex-1 min-h-0 overflow-y-auto overscroll-behavior-none px-4 pb-[calc(var(--safe-area-bottom,0px)+12px)]"
          >
            <div v-for="row in rows" :key="row.nodeId" class="group-card mb-3">
              <div class="px-3 py-3">
                <!-- 首行：状态点 + 设备名 + 短指纹 + 标签 -->
                <div class="flex items-center gap-2 min-w-0">
                  <span class="ft-dev-dot flex-shrink-0" :class="dotClass(row)"></span>
                  <span class="ft-dev-name text-[var(--mobile-text-primary)] truncate">
                    {{ row.deviceName }}
                  </span>
                  <span class="ft-dev-fp flex-shrink-0">{{ shortFingerprint(row.nodeId) }}</span>
                  <span
                    v-if="row.isActive"
                    class="flex-shrink-0 ft-chip ft-color-active"
                  >
                    {{ t('transfer.devices.activeCurrent') }}
                  </span>
                  <span v-else-if="!row.fileTransfer" class="flex-shrink-0 ft-chip badge-zinc">
                    {{ t('transfer.devices.capNone') }}
                  </span>
                </div>

                <!-- 元信息（三态 + 地址） -->
                <p class="ft-dev-meta mt-1 truncate">{{ metaText(row) }}</p>

                <!-- 拨号终态反馈：拒绝 / 不可达（行内错误，非全局 toast） -->
                <p v-if="row.dialError" class="ft-dev-error mt-1.5">
                  {{
                    row.dialError === 'denied'
                      ? t('transfer.devices.denied')
                      : t('transfer.devices.unreachable')
                  }}
                </p>

                <!-- 行操作：44px 最小触控目标；连接中显示占位文案防重复发起 -->
                <div class="mt-2 flex gap-2">
                  <button
                    v-if="row.status === 'connected' && !row.isActive"
                    class="ft-dev-action ft-btn-neutral"
                    @click="emit('set-active', row.nodeId)"
                  >
                    {{ t('transfer.devices.setActive') }}
                  </button>
                  <button
                    v-if="row.status === 'connected'"
                    class="ft-dev-action ft-dev-action--danger"
                    @click="emit('disconnect', row.nodeId)"
                  >
                    {{ t('transfer.devices.disconnect') }}
                  </button>
                  <span v-else-if="row.status === 'connecting'" class="ft-dev-connecting-hint">
                    {{ t('transfer.devices.connecting') }}
                  </span>
                  <button
                    v-else
                    class="ft-dev-action ft-btn-accent"
                    :disabled="!row.fileTransfer"
                    @click="emit('connect', row.nodeId)"
                  >
                    {{ t('transfer.devices.connect') }}
                  </button>
                </div>
              </div>
            </div>

            <!-- 信任提示（列表尾部一次性说明，非逐行重复） -->
            <p class="ft-dev-hint px-1 pt-1 pb-2">
              {{ t('transfer.devices.trustHint') }}
            </p>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* 面板最大高度：小屏防溢出（与 TaskQueueSheet 同值） */
.ft-dev-panel {
  max-height: 78dvh;
}

/* 标题 / 副标题：流式字号 */
.ft-dev-title {
  font-size: clamp(0.9375rem, 1rem + (100vw - 360px) / 800, 1.0625rem);
  font-weight: 600;
}

.ft-dev-subtitle {
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
}

/* 探索发现按钮：头部行尾，44px 最小触控高度，按压反馈 opacity */
.ft-dev-scan-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  min-height: 2.75rem;
  padding: 0.5rem 0.625rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  color: var(--mobile-accent);
  -webkit-tap-highlight-color: transparent;
  transition: opacity 0.15s ease;
}

.ft-dev-scan-btn:active {
  opacity: 0.8;
}

.ft-dev-scan-btn:disabled {
  opacity: 0.45;
}

/* 探索发现图标 */
.ft-dev-scan-ico {
  width: 0.875rem;
  height: 0.875rem;
  flex-shrink: 0;
}

/* 扫描中 spinner：仅 transform 动画（GPU 合成），尊重减弱动效偏好 */
.ft-dev-scan-spin {
  width: 0.875rem;
  height: 0.875rem;
  flex-shrink: 0;
  animation: ft-dev-scan-rotate 0.9s linear infinite;
}

@keyframes ft-dev-scan-rotate {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .ft-dev-scan-spin {
    animation: none;
  }
}

/* 空态文字 */
.ft-dev-empty {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  color: var(--mobile-text-muted);
}

/* 状态点：圆点语义色；握手中琥珀呼吸动画（尊重减弱动效偏好） */
.ft-dev-dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 9999px;
}

.ft-dev-dot--online {
  background: var(--mobile-success);
}

/* 在线未连接（可达但无传输会话）：降饱和绿，与实心绿区分（与桌面同构） */
.ft-dev-dot--reachable {
  background: var(--mobile-success);
  opacity: 0.45;
}

.ft-dev-dot--connecting {
  background: var(--mobile-warning);
  animation: ft-dev-dot-breathe 1.6s ease-in-out infinite;
}

.ft-dev-dot--offline {
  background: var(--mobile-border-hover);
}

@keyframes ft-dev-dot-breathe {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

@media (prefers-reduced-motion: reduce) {
  .ft-dev-dot--connecting {
    animation: none;
  }
}

/* 设备名 / 短指纹 / 元信息：流式字号，指纹与元信息弱化色 */
.ft-dev-name {
  font-size: clamp(0.8125rem, 0.875rem + (100vw - 360px) / 800, 0.9375rem);
  font-weight: 500;
}

.ft-dev-fp {
  font-size: clamp(0.625rem, 0.6875rem + (100vw - 360px) / 800, 0.75rem);
  color: var(--mobile-text-disabled);
  font-family: ui-monospace, monospace;
}

.ft-dev-meta {
  margin: 0;
  padding-left: 1rem; /* 与设备名对齐（跳过状态点宽度 + gap） */
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-text-muted);
  font-variant-numeric: tabular-nums;
}

/* 行内错误文案：error 色（不占整卡色条块，轻量呈现） */
.ft-dev-error {
  margin: 0;
  margin-left: 1rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  line-height: 1.45;
  color: var(--mobile-error);
}

/* 行操作按钮：44px 最小触控高度，按压反馈 opacity（与队列操作按钮同语言） */
.ft-dev-action {
  min-height: 2.75rem;
  padding: 0.5rem 0.875rem;
  border-radius: 0.5rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  font-weight: 500;
  flex-shrink: 0; /* 行内空间不足时禁止压缩按钮（与桌面 ft-mini-btn 同规则） */
  transition: opacity 0.15s ease;
}

.ft-dev-action:active {
  opacity: 0.8;
}

.ft-dev-action:disabled {
  opacity: 0.45;
}

/* 断开按钮：危险语义（error tint 描边，不用实底红降低攻击性） */
.ft-dev-action--danger {
  color: var(--mobile-error);
  background: color-mix(in srgb, var(--mobile-error) 10%, transparent);
}

/* 连接中提示：占位文案（非按钮），保持行高稳定防抖动 */
.ft-dev-connecting-hint {
  display: inline-flex;
  align-items: center;
  min-height: 2.75rem;
  padding: 0.5rem 0.25rem;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  color: var(--mobile-warning);
}

/* 尾部信任提示 */
.ft-dev-hint {
  margin: 0;
  font-size: clamp(0.6875rem, 0.75rem + (100vw - 360px) / 800, 0.8125rem);
  line-height: 1.5;
  color: var(--mobile-text-disabled);
}
</style>
