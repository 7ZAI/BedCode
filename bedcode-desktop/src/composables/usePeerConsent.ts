/**
 * 首连确认弹窗编排 — 对等网络信任层前端桥（issue 04）
 *
 * 监听后端 `peer-consent-requested` 事件并维护单条弹窗队列；迁移规则在此判定：
 * 请求方设备名命中终端配对名单 → 视为同一用户的设备，静默自动互信不弹窗
 * （ADR 0002「终端配对记录即同一用户的证明」）。匹配键用设备名是 v1 的务实
 * 选择——peer-net 节点身份与终端设备指纹刻意分离（决策 D2），跨身份强映射属
 * 后续票。30s 无应答主动拒绝，与 crate `CONFIRM_TIMEOUT` 同值——先行结算
 * 释放闸门槽位，拨入方立即收到 Denied 而非等满超时。
 */
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

/** 后端 peer-consent-requested 事件载荷 */
export interface PeerConsentRequest {
  requestId: string
  /** 完整节点 ID */
  nodeId: string
  /** 短指纹（前 8 位） */
  fingerprintShort: string
  /** 设备名（发现缓存解析；离线/缺失为 null） */
  deviceName: string | null
}

/** 弹窗超时：与 crate transport::CONFIRM_TIMEOUT 保持一致 */
export const PEER_CONSENT_TIMEOUT_MS = 30_000

/**
 * 迁移规则匹配（纯函数）：请求方设备名是否命中已配对终端设备名单
 *
 * 无名记录不参与匹配（宁可多弹一次窗，不可误信陌生节点）。
 */
export function matchesTerminalPairedDevice(
  deviceName: string | null,
  pairedNames: readonly string[],
): boolean {
  return !!deviceName && pairedNames.includes(deviceName)
}

// ==================== 模块级共享状态（跨组件单例） ====================

/** 当前展示的确认请求；null = 无弹窗 */
const currentRequest = ref<PeerConsentRequest | null>(null)

/** 最近一次因终端配对迁移规则被自动互信的设备名（宿主层提示用，非弹窗路径） */
const autoTrustedName = ref<string | null>(null)

let queue: PeerConsentRequest[] = []
let timer: ReturnType<typeof setTimeout> | null = null
let unlisten: UnlistenFn | null = null
let started = false

/** 配对名单获取器：start() 时注入的模块级引用（accept/deny 与队列推进共用） */
let pairedNamesGetter: (() => Promise<string[]>) | null = null

function loadPairedNames(): Promise<string[]> {
  return pairedNamesGetter ? pairedNamesGetter() : Promise.resolve([])
}

function clearTimer(): void {
  if (timer) {
    clearTimeout(timer)
    timer = null
  }
}

async function respond(requestId: string, accepted: boolean): Promise<boolean> {
  try {
    return await invoke<boolean>('respond_peer_consent', { requestId, accepted })
  } catch (error) {
    console.error('[PeerConsent] respond failed:', error)
    return false
  }
}

/** 展示待确认项（调用方已完成迁移规则判定） */
function present(next: PeerConsentRequest): void {
  currentRequest.value = next
  clearTimer()
  timer = setTimeout(() => {
    // 超时按拒绝结算：立即释放闸门 pending 槽位（与 crate CONFIRM_TIMEOUT 同值）
    void answer(false)
  }, PEER_CONSENT_TIMEOUT_MS)
}

function showNext(): Promise<void> {
  const next = queue.shift()
  if (!next) return Promise.resolve()
  present(next)
  return Promise.resolve()
}

async function answer(accepted: boolean): Promise<void> {
  const request = currentRequest.value
  if (!request) return
  clearTimer()
  currentRequest.value = null
  await respond(request.requestId, accepted)
  await showNext()
}

/**
 * 首连确认控制器：应用生命周期内调用一次 `start`，弹窗宿主组件持有其余方法
 *
 * `getPairedNames` 返回已配对终端设备名单（桌面取 deviceStore、移动取
 * localStorage），每次判定时实时拉取以反映最新配对状态；引用存于模块级，
 * 后续 accept/deny/超时/队列推进全部复用同一来源。
 */
export function usePeerConsent() {
  async function start(getPairedNames: () => Promise<string[]>): Promise<void> {
    if (started) return
    started = true
    pairedNamesGetter = getPairedNames
    unlisten = await listen<PeerConsentRequest>('peer-consent-requested', async (event) => {
      const request = event.payload
      // 迁移规则先行：已配对终端设备立即放行，即使当前有其他弹窗在展示
      // （不占用用户交互，也不入队等待）；名单拉取失败按「无配对」走人工确认
      try {
        const names = await loadPairedNames()
        if (matchesTerminalPairedDevice(request.deviceName, names)) {
          autoTrustedName.value = request.deviceName
          console.info('[PeerConsent] terminal-paired device auto-trusted:', request.deviceName)
          void respond(request.requestId, true)
          return
        }
      } catch (error) {
        console.error('[PeerConsent] load paired names failed:', error)
      }
      // 已有弹窗在展示则排队，保证用户一次只面对一个确认
      if (currentRequest.value || queue.length > 0) {
        queue.push(request)
        return
      }
      present(request)
    })
  }

  /** 接受当前请求并推进队列 */
  async function accept(): Promise<void> {
    await answer(true)
  }

  /** 拒绝当前请求并推进队列（关闭弹窗等同拒绝） */
  async function deny(): Promise<void> {
    await answer(false)
  }

  return { currentRequest, autoTrustedName, start, accept, deny }
}

// ==================== 测试辅助 ====================

/** 重置模块级状态（仅测试用：用例间隔离共享单例） */
export function _resetPeerConsentForTest(): void {
  clearTimer()
  currentRequest.value = null
  autoTrustedName.value = null
  queue = []
  unlisten?.()
  unlisten = null
  started = false
  pairedNamesGetter = null
}
