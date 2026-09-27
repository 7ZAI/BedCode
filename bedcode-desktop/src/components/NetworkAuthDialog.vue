<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="request"
        class="fixed inset-0 z-[9999] flex items-center justify-center p-4"
        @click.self="respond('deny')"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>

        <!-- Dialog -->
        <div
          class="relative w-full max-w-sm rounded-card shadow-2xl border bg-card border-[var(--border)]"
        >
          <!-- Header -->
          <div class="px-6 py-4 border-b border-[var(--border)]">
            <h3 class="text-lg font-semibold text-[var(--text-primary)]">
              {{ t('desktop.plugin.netAuthTitle') }}
            </h3>
          </div>

          <!-- Body -->
          <div class="p-6 space-y-4">
            <p class="text-sm text-[var(--text-secondary)]">
              {{ t('desktop.plugin.netAuthRequest', { plugin: request.pluginId }) }}
            </p>

            <!-- Origin 展示：只显示归一化 origin（宿主不投 path / query，token 不会上屏） -->
            <div class="space-y-1">
              <span class="text-xs text-[var(--text-tertiary)]">
                {{ t('desktop.plugin.netAuthOrigin') }}
              </span>
              <div
                class="p-2 rounded-input bg-[var(--bg-input)] text-xs text-[var(--text-primary)] break-all wb-mono"
              >
                {{ request.origin }}
              </div>
            </div>

            <!-- 授权含义说明：同意 = 此地址此后免询问（否则用户只能靠猜）。
                 「总是询问」档下不落记录，文案必须跟着变——说一套做一套比不说更糟 -->
            <p class="text-xs text-[var(--text-tertiary)]">
              {{
                request?.remembers === false
                  ? t('desktop.plugin.netAuthScopeOnce')
                  : t('desktop.plugin.netAuthScope')
              }}
            </p>
          </div>

          <!-- Footer：三个决定（宿主只认这三态，票 05 固定枚举） -->
          <div
            class="flex items-center justify-end gap-2 px-6 py-4 border-t border-[var(--border)]"
          >
            <button
              class="px-3 h-9 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-tertiary)] hover:bg-[var(--bg-input)] transition-colors duration-200"
              @click="respond('deny_always')"
            >
              {{ t('desktop.plugin.netAuthDenyAlways') }}
            </button>
            <button
              class="px-3 h-9 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors duration-200"
              @click="respond('deny')"
            >
              {{ t('desktop.plugin.netAuthDeny') }}
            </button>
            <button
              class="px-4 h-9 rounded-btn text-sm font-medium bg-brand text-[var(--color-primary-contrast)] hover:bg-[var(--color-primary-hover)] transition-colors duration-200"
              @click="respond('allow_once')"
            >
              {{ t('desktop.plugin.netAuthAllow') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * NetworkAuthDialog — 网络出站授权弹窗（授权策略增强 · 票 05）
 *
 * 监听宿主事件 `plugin:network-auth-request`，把三态决定经
 * `plugin_network_auth_respond` 回给宿主（`allow_once` / `deny` / `deny_always`）。
 *
 * ## 为什么与 `FsAuthDialog` 分开而不是复用
 * 两条判定面的**决定形状不同**（fs 是 `allowed + remember` 双布尔，网络是固定三态
 * 枚举），且文案、载荷字段（path 列表 vs 归一化 origin）都不同；复用一个组件会逼出
 * 一堆 `if (kind === 'fs')` 分支。事件名也分开，避免两套弹窗在同一时刻互相覆盖。
 *
 * ## 样式口径
 * 完全复制 `FsAuthDialog` 的 modal 蓝图（`BLUEPRINTS.md` Overlay Pattern）：
 * `<Teleport to="body">` + `z-[9999]`（safe-stack Emergency 层）+ `bg-black/50
 * backdrop-blur-sm` 桌面遮罩 + token 化色彩（`--bg-*` / `--text-*` / `--border`）。
 * 沿用 Emergency 层的原因与 fs 侧一致：授权确认必须悬浮在插件启停遮罩（z-50）之上，
 * 否则用户在遮罩显示期间看不见授权框，30s 超时后请求被拒（回归见本组件的测试）。
 */
import { ref, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { pluginNetworkAuthRespond, type NetworkAuthDecision } from '@/plugin/commands'
import { logger } from '@/utils/frontendLogger'

const { t } = useI18n()

/** 宿主弹窗事件载荷（与 `network_auth::prompt` 发出的 json 逐字对齐） */
interface NetworkAuthRequest {
  requestId: string
  pluginId: string
  /** 归一化 origin（`scheme://host:port`；宿主不投 path / query） */
  origin: string
  /**
   * 点「允许」会不会落成授权记录（= 弹出时档位是否读记录，票 06）
   *
   * 「总是询问」档为 `false`：那一次允许只放行当前这批请求，下次（合并窗口过后）
   * 还会再问。弹窗说明必须照实说，否则用户以为「允许了」却在下次又被问。
   * 字段缺失（宿主早于本字段的版本）按 `true` 处理——保持旧文案，不凭空改变行为。
   */
  remembers?: boolean
}

const request = ref<NetworkAuthRequest | null>(null)

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<NetworkAuthRequest>('plugin:network-auth-request', (event) => {
    request.value = event.payload
  })
})

onUnmounted(() => {
  unlisten?.()
})

/** 回传决定并关闭弹窗 */
async function respond(decision: NetworkAuthDecision): Promise<void> {
  if (!request.value) return
  const { requestId } = request.value
  request.value = null
  try {
    await pluginNetworkAuthRespond(requestId, decision)
  } catch (e) {
    // 应答失败（宿主拒绝代答 / 询问已超时失效）：弹窗已关，宿主侧按拒绝收场，
    // 只留日志——用户已经做过决定，再弹错误提示只会让人以为还能改
    logger.error('[NetworkAuthDialog] 应答失败', e)
  }
}
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > :last-child,
.modal-leave-to > :last-child {
  transform: scale(0.95);
}
</style>
