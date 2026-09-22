<template>
  <Teleport to="body">
    <Transition name="overlay">
      <div
        v-if="plugin"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="!approving && emit('close')"
      >
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm"></div>
        <div
          class="relative w-full max-w-md bg-[var(--bg-card)] border border-[var(--border)] rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh]"
        >
          <!-- ==================== 标题与说明 ==================== -->
          <div class="p-6 pb-3">
            <div class="flex items-center gap-2.5 mb-2">
              <span
                class="w-4 h-4 flex items-center justify-center text-[var(--color-primary)] shrink-0"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M12 3l7 3v6c0 4.418-2.91 8.13-7 9-4.09-.87-7-4.582-7-9V6l7-3z"
                  />
                </svg>
              </span>
              <h3
                class="text-[calc(14px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
              >
                {{ $t('desktop.plugin.approve.title') }}
              </h3>
            </div>
            <p
              class="text-[calc(12px*var(--ui-scale))] leading-relaxed text-[var(--text-secondary)]"
            >
              {{ $t('desktop.plugin.approve.desc', { name: plugin.name }) }}
            </p>
          </div>

          <!-- ==================== 权限清单（高危位红色强调 + 后果文案） ==================== -->
          <div class="flex-1 overflow-y-auto px-6">
            <div
              v-if="rows.length === 0"
              class="py-2 text-[calc(12px*var(--ui-scale))] text-[var(--text-tertiary)]"
            >
              {{ $t('desktop.plugin.approve.empty') }}
            </div>
            <div
              v-else
              class="rounded-[8px] border border-[var(--border)] divide-y divide-[var(--border)] overflow-hidden"
            >
              <div v-for="row in rows" :key="row.perm" class="flex items-start gap-3 px-3 py-2.5">
                <span
                  class="w-4 h-4 flex items-center justify-center text-xs shrink-0 mt-0.5"
                  >{{ row.meta.emoji }}</span
                >
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span
                      class="text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)]"
                    >
                      {{ row.meta.title }}
                    </span>
                    <span
                      v-if="row.highRisk"
                      class="px-1.5 py-0.5 rounded-[4px] text-[calc(10px*var(--ui-scale))] font-medium bg-red-50 dark:bg-red-500/10 text-red-600 dark:text-red-400 shrink-0"
                    >
                      {{ $t('desktop.plugin.approve.highRiskLabel') }}
                    </span>
                  </div>
                  <div
                    class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)] mt-0.5"
                  >
                    {{ row.meta.desc }}
                  </div>
                  <div
                    v-if="row.meta.risk"
                    class="text-[calc(11px*var(--ui-scale))] text-red-600 dark:text-red-400 mt-0.5"
                  >
                    {{ row.meta.risk }}
                  </div>
                </div>
                <span
                  class="wb-mono text-[calc(10px*var(--ui-scale))] text-[var(--text-tertiary)] shrink-0 mt-0.5"
                  >{{ row.perm }}</span
                >
              </div>
            </div>
          </div>

          <!-- ==================== 操作 ==================== -->
          <div class="flex gap-3 px-6 py-4">
            <button
              class="flex-1 h-8 rounded-[6px] border border-[var(--border)] text-[calc(12px*var(--ui-scale))] font-medium text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50"
              :disabled="approving"
              @click="emit('close')"
            >
              {{ $t('desktop.plugin.approve.cancel') }}
            </button>
            <button
              class="flex-1 h-8 rounded-[6px] text-[calc(12px*var(--ui-scale))] font-medium text-[var(--color-primary-contrast)] bg-[var(--color-primary)] hover:opacity-90 disabled:opacity-50 transition-opacity"
              :disabled="approving"
              @click="confirmApprove"
            >
              {{ approving ? $t('desktop.plugin.approve.approving') : $t('desktop.plugin.approve.confirm') }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * PluginApprovalDialog - 插件权限审批弹层（ADR 0020 / 审计票 03）
 *
 * 用户 zip 安装的插件在**首次启用前**必须人工确认权限清单：本弹层把 manifest
 * 声明的权限逐条列出（高危位红标 + 后果文案），确认后由宿主写入批准记录并把
 * 批准与插件目录内容哈希绑定（内容变化即撤销批准）。
 *
 * 组件只负责交互与提交，不做权限裁决：批准集由 Rust 端计算（词汇表过滤 + 哈希
 * 钉扎），失败原因原样透出 toast。
 *
 * 视觉沿用插件详情页既有弹层蓝图（卸载确认弹窗：Teleport + backdrop + 卡片）。
 */
import { computed, ref } from 'vue'
import { pluginApprove } from '@/plugin/commands'
import { getPermissionMeta, isHighRiskPermission } from '@/plugin/contributionKinds'
import { useToast } from '@/composables/useToast'
import { logger } from '@/utils/frontendLogger'
import i18n from '@/locales'
import type { PluginInfo } from '@/plugin/types'

const props = defineProps<{
  /** 待审批插件；null 表示不渲染（调用方用 v-if 亦可） */
  plugin: PluginInfo | null
}>()

const emit = defineEmits<{
  /** 请求关闭（遮罩点击 / 取消；提交中不触发） */
  close: []
  /** 批准成功（参数为插件 id，调用方据此决定是否继续启用） */
  approved: [pluginId: string]
}>()

const toast = useToast()
const t = i18n.global.t

const approving = ref(false)

/** 权限行（元数据 + 高危标记一次算好，避免模板里重复查表） */
const rows = computed(() =>
  (props.plugin?.permissions ?? []).map((perm) => ({
    perm,
    meta: getPermissionMeta(perm),
    highRisk: isHighRiskPermission(perm),
  })),
)

/** 确认批准：成功即通知调用方（关闭 + 可选继续启用），失败保留弹层供重试 */
async function confirmApprove(): Promise<void> {
  const target = props.plugin
  if (!target || approving.value) return
  approving.value = true
  try {
    const approved = await pluginApprove(target.id)
    logger.info(
      `[PluginApprovalDialog] plugin ${target.id} approved, ${approved.length} permission(s) effective`,
    )
    toast.success(t('desktop.plugin.approve.success', { name: target.name }))
    emit('approved', target.id)
  } catch (e: any) {
    logger.error(`[PluginApprovalDialog] pluginApprove(${target.id}) failed:`, e)
    toast.error(t('desktop.plugin.approve.failed', { error: e?.message || String(e) }))
  } finally {
    approving.value = false
  }
}
</script>

<style scoped>
.overlay-enter-active,
.overlay-leave-active {
  transition: opacity 0.2s ease;
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}
</style>
