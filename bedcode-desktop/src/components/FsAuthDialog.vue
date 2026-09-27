<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="request"
        class="fixed inset-0 z-[9999] flex items-center justify-center p-4"
        @click.self="deny"
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
              {{ t('desktop.plugin.fsAuthTitle') }}
            </h3>
          </div>

          <!-- Body -->
          <div class="p-6 space-y-4">
            <!-- Description -->
            <p class="text-sm text-[var(--text-secondary)]">
              {{
                request.origin === 'picker'
                  ? t('desktop.plugin.fsAuthPickerRequest', {
                      plugin: request.pluginId,
                      operation: operationLabel,
                    })
                  : t('desktop.plugin.fsAuthRequest', {
                      plugin: request.pluginId,
                      operation: operationLabel,
                    })
              }}
            </p>

            <!-- Path display -->
            <div class="space-y-1">
              <span class="text-xs text-[var(--text-tertiary)]">
                {{
                  pathCount > 1
                    ? t('desktop.plugin.fsAuthPaths', { count: pathCount })
                    : t('desktop.plugin.fsAuthPath')
                }}
              </span>
              <div
                v-if="pathCount > 1"
                class="max-h-40 overflow-y-auto p-2 rounded-input bg-[var(--bg-input)] text-xs text-[var(--text-primary)] space-y-1"
              >
                <div v-for="p in paths" :key="p" class="break-all font-mono leading-relaxed">
                  {{ p }}
                </div>
              </div>
              <div
                v-else
                class="p-2 rounded-input bg-[var(--bg-input)] text-xs text-[var(--text-primary)] break-all font-mono"
              >
                {{ paths[0] || request.path }}
              </div>
            </div>

            <!-- 授权落账范围（选择器场景按目录落账：说清「记住」到底记住了什么） -->
            <p
              v-if="request.grantScope === 'directory'"
              class="text-xs text-[var(--text-tertiary)]"
            >
              {{ t('desktop.plugin.fsAuthGrantScopeDir', { operation: operationLabel }) }}
            </p>

            <!-- Remember checkbox（票 02：「记住」只覆盖本次请求的操作集）
                 票 03：「总是询问」档跳过授权记录，该档不出现「记住」——写了也没人读，
                 与档位语义正面冲突（spec §6.3） -->
            <label
              v-if="offersRemember"
              class="flex items-center gap-2 cursor-pointer select-none"
            >
              <input
                v-model="remember"
                type="checkbox"
                class="w-4 h-4 rounded accent-[var(--color-primary)]"
              />
              <span class="text-sm text-[var(--text-secondary)]">
                {{ t('desktop.plugin.fsAuthRemember', { operation: operationLabel }) }}
              </span>
            </label>
          </div>

          <!-- Footer（票 03 三态：允许本次 / 拒绝 / 以后都拒绝；「默认」档的允许受「记住」影响） -->
          <div
            class="flex items-center justify-end gap-3 px-6 py-4 border-t border-[var(--border)]"
          >
            <button
              class="px-4 h-9 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors duration-200"
              @click="deny"
            >
              {{ t('desktop.plugin.fsAuthDeny') }}
            </button>
            <button
              data-testid="fs-auth-deny-always"
              class="px-4 h-9 rounded-btn text-sm font-medium bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:bg-[var(--bg-input)] transition-colors duration-200"
              @click="denyAlways"
            >
              {{ t('desktop.plugin.fsAuthDenyAlways') }}
            </button>
            <button
              class="px-4 h-9 rounded-btn text-sm font-medium bg-brand text-[var(--color-primary-contrast)] hover:bg-[var(--color-primary-hover)] transition-colors duration-200"
              @click="allow"
            >
              {{
                offersRemember
                  ? t('desktop.plugin.fsAuthAllow')
                  : t('desktop.plugin.fsAuthAllowOnce')
              }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * FsAuthDialog — 文件系统授权弹窗
 *
 * 监听 plugin:fs-auth-request 事件，显示授权请求弹窗，
 * 用户选择后通过 plugin_fs_auth_respond Tauri command 回调宿主
 *
 * 两种来源共用本弹窗（宿主同一个事件）：
 * - `origin: 'fs'`：插件直接请求访问某路径（activate 期的目录授权等）；
 * - `origin: 'picker'`：宿主系统文件选择器已返回、这些路径是**用户刚亲手选中**的
 *   （`host-platform.pick-*` 的结果门），文案要说明「为什么突然又问一次」；
 * `grantScope: 'directory'` 时额外说明「记住」按**所在目录**落账——授权范围必须
 * 让用户看得见，否则「勾了记住」的实际含义只有宿主知道。
 *
 * 三态应答（票 03）：允许本次 / 拒绝 / 以后都拒绝；「默认」档另有「记住」勾选
 * （允许并记住）。**决定集由宿主随事件下发的 `strategy` 决定**（弹出时快照）：
 * 「总是询问」档跳过授权记录，该档不出现「记住」（写了也没人读，spec §6.3），
 * 弹窗不自行按当前档位二次判断——等待应答期间用户改了档位，这个弹窗仍按弹出时
 * 的口径渲染与落账（spec §8.1）。
 *
 * 层级用 safe-stack Emergency 层（z-[9999]）：授权确认必须悬浮于任意
 * overlay 之上可交互——插件启停遮罩 LoadingOverlay（z-50）显示期间，
 * 插件 activate 内的 fs_request_auth（ADR 0007 激活期目录授权）会弹本
 * 弹窗；同层级（z-50）时后挂载的遮罩会按 DOM 顺序覆盖本弹窗，导致
 * 用户看不见授权框、30s 超时后授权被拒（回归测试见 FsAuthDialog.test.ts）
 */
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { pluginFsAuthRespond, type FsAuthDecision } from '@/plugin/commands'
import { normalizeStrategy } from '@/utils/authPolicy'

const { t } = useI18n()

interface FsAuthRequest {
  requestId: string
  pluginId: string
  path?: string
  /** 批量授权：未授权路径数组（桌面 1.0 后新增，优先于 path） */
  paths?: string[]
  operation: string
  /** 请求来源：fs = 插件直接请求访问；picker = 用户刚在系统选择器里选中 */
  origin?: 'fs' | 'picker'
  /** 「记住」的落账粒度：directory = 按所在目录记（同目录其它文件不再询问） */
  grantScope?: 'exact' | 'directory'
  /** 弹出时的策略档位（票 03；缺字段 / 值未识别按默认档，与宿主读面兜底同向） */
  strategy?: string
}

const request = ref<FsAuthRequest | null>(null)
const remember = ref(true)

/** 弹出时的档位（决定本次弹窗给不给「记住」） */
const strategy = computed(() => normalizeStrategy(request.value?.strategy))

/**
 * 是否提供「记住」：档位跳过授权记录时不提供
 *
 * 与宿主 `PendingRequest::offers_remember` 同一判据（同一份档位值），
 * 两处不得各写一套——前端多给一个按钮就会落到宿主侧「越界的 allow_remember
 * 降级为一次性放行」那条兜底上，用户会以为记住生效了。
 */
const offersRemember = computed(() => strategy.value !== 'always_ask')

/** 待展示路径列表（批量事件用 paths，兼容旧事件回退 path） */
const paths = computed(() => {
  if (!request.value) return []
  return request.value.paths?.length ? request.value.paths : [request.value.path || '']
})

const pathCount = computed(() => paths.value.length)

/**
 * 操作集展示（票 02）：宿主按本次请求的能力集发 `read` / `write` / `read+write`
 * （`FsOps::as_wire_str`）；未知值按「读取」处理是危险的默认，故一律回落读——但
 * 未知值只可能来自宿主自身的新版本，与前端同包更新，不会出现单边漂移。
 */
const operationLabel = computed(() => {
  const operation = request.value?.operation
  if (operation === 'write') return t('desktop.plugin.fsAuthWrite')
  if (operation === 'read+write') return t('desktop.plugin.fsAuthReadWrite')
  return t('desktop.plugin.fsAuthRead')
})

let unlisten: UnlistenFn | null = null

onMounted(async () => {
  unlisten = await listen<FsAuthRequest>('plugin:fs-auth-request', (event) => {
    request.value = event.payload
    remember.value = true
  })
})

onUnmounted(() => {
  unlisten?.()
})

async function allow() {
  if (!request.value) return
  const { requestId } = request.value
  // 「记住」= 允许并记住（落 allow 记录）；否则一次性放行
  const decision: FsAuthDecision =
    offersRemember.value && remember.value ? 'allow_remember' : 'allow_once'
  request.value = null
  await pluginFsAuthRespond(requestId, decision)
}

async function deny() {
  if (!request.value) return
  const { requestId } = request.value
  request.value = null
  await pluginFsAuthRespond(requestId, 'deny')
}

/** 以后都拒绝：落一条 deny 记录，该目标与其子树后续被直接拒绝（spec §8.4） */
async function denyAlways() {
  if (!request.value) return
  const { requestId } = request.value
  request.value = null
  await pluginFsAuthRespond(requestId, 'deny_always')
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
