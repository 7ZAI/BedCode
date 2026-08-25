/**
 * File Transfer 插件领域 mock（dev-shell 专用，纯通用接线）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册
 * 的 handler。本模块注册对等域与传输域的命令 handler 骨架（query-peer /
 * dial-peer / list-remote / get-settings 等），并模拟事件推送
 * （devices-changed / connection-changed），使插件在 dev-shell 中可完整演示
 * 三态、握手、行内错误与活跃切换。
 *
 * 本模块不包含任何具体业务 mock 数据：全部演示种子由插件工程持有
 * （入口导出 devMock，SDK PluginDevMock 协议的 peer / transfer 子域及本地
 * 扩展字段），按 pluginId 经 getDevMock 取种子驱动命令返回值与事件；
 * 未导出种子的插件不受影响（各子域回退空态）。
 */
import { emitDevEvent } from './session'
import type { PeerDevMock, PluginContext, PluginDevMock } from '../../../src/types'
import { pushLog } from '../registry'

/** 种子设备条目（与 SDK PeerDevMock.devices 一致；本地重声明避免隐式 any） */
interface MockDevice {
  nodeId: string
  deviceName: string
  addr?: string
  fileTransfer?: boolean
}

/**
 * consent 演示种子（插件 devMock 导出的本地扩展字段，与插件侧 ConsentDevSeed
 * 形状对齐；SDK PluginDevMock 协议尚未收录，本地声明避免类型越界）
 */
interface MockConsentSeed {
  pairedDevices: string[]
  requests: Array<{
    delayMs: number
    request: {
      requestId: string
      nodeId: string
      fingerprintShort?: string
      deviceName?: string | null
    }
  }>
}

/** 可信对端种子条目（插件 devMock 导出的本地扩展字段，SDK 协议未收录） */
interface MockTrustedSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  addedAt: string
}

/** 定时器句柄（setTimeout/setInterval 在浏览器返回 number） */
const timers: number[] = []

/** 共享目录条目（宿主 wire DTO 形状，含 SAF tree_uri） */
interface MockSharedDir {
  id: string
  name: string
  tree_uri: string
  builtin?: boolean
}

/**
 * 注册对等域 + 传输域命令 handler 并推送初始快照（loader 在 activate 前调用）
 *
 * @param context 插件 mock 上下文
 * @param mock 插件领域种子（getDevMock(pluginId)）；缺省时各子域演示空态
 * @param pluginId 插件 ID（pushLog 归属标签）
 */
export function registerFileTransferMock(
  context: PluginContext,
  mock: PluginDevMock | undefined,
  pluginId: string,
): void {
  const peerSeed = mock?.peer as (PeerDevMock & { consent?: MockConsentSeed; trusted?: unknown }) | undefined
  const devices: MockDevice[] = peerSeed?.devices ?? []
  const connectedNodes = new Set<string>(peerSeed?.connectedNodeIds ?? [])
  let activePeerId = peerSeed?.activeNodeId ?? ''
  const dialBehavior = peerSeed?.dialBehavior ?? {}
  const dialLatencyMs = peerSeed?.dialLatencyMs ?? 800

  // ==================== 对等域事件推送 ====================

  /** 连接态增量：维护已连接集合并推 connection-changed（{ nodeId, connected } 契约） */
  function emitConnection(nodeId: string, connected: boolean): void {
    const device = devices.find((d) => d.nodeId === nodeId)
    if (connected) {
      if (!device || connectedNodes.has(nodeId)) return
      connectedNodes.add(nodeId)
    } else {
      if (!connectedNodes.has(nodeId)) return
      connectedNodes.delete(nodeId)
    }
    emitDevEvent('plugin:file-transfer:connection-changed', {
      nodeId,
      connected,
      deviceName: device?.deviceName ?? null,
    })
  }

  /** 发现快照 + 连接态全量补发（dev-shell 事件总线不重放历史，晚订阅者靠它追平） */
  function pushPeerSnapshot(): void {
    emitDevEvent(
      'plugin:file-transfer:devices-changed',
      devices.map((d) => ({ ...d })),
    )
    for (const nodeId of connectedNodes) {
      const device = devices.find((d) => d.nodeId === nodeId)
      emitDevEvent('plugin:file-transfer:connection-changed', {
        nodeId,
        connected: true,
        deviceName: device?.deviceName ?? null,
      })
    }
  }

  // ==================== 对等域命令 handler ====================

  // 发现快照：执行时同步补发订阅追平事件（组件挂载晚于插件激活，
  // 初始推送会错失；query-peer 由 usePeerDevices/useTasks 挂载后主动拉取）
  context.commands.register('file-transfer.query-peer', () => {
    pushPeerSnapshot()
    return devices.map((d) => ({ ...d }))
  })
  // 旧契约：可传输对端列表 + 活跃对端（usePeerDevices.refresh 拉活跃态用）
  context.commands.register('file-transfer.list-peers', () => ({
    peers: devices
      .filter((d) => d.fileTransfer !== false)
      .map((d) => ({ deviceId: d.nodeId, name: d.deviceName })),
    activePeerId,
  }))
  context.commands.register('file-transfer.set-active-peer', (args: any) => {
    const id = args?.peerId
    if (typeof id === 'string' && (!id || devices.some((d) => d.nodeId === id))) {
      activePeerId = id
    }
    return { activePeerId }
  })
  // 对等拨号：按 seed.dialBehavior 返回终态；connected 时同步推连接态事件
  context.commands.register('file-transfer.dial-peer', (args: any) => {
    const nodeId: string = args?.nodeId ?? ''
    const device = devices.find((d) => d.nodeId === nodeId)
    if (!device || !device.fileTransfer || connectedNodes.has(nodeId)) {
      return Promise.reject(new Error(`dial failed: cannot dial "${nodeId}"`))
    }
    const behavior = dialBehavior[nodeId] ?? 'unreachable'
    return new Promise((resolve, reject) => {
      timers.push(
        setTimeout(() => {
          if (behavior === 'connected') {
            emitConnection(nodeId, true)
            resolve({ status: 'connected', deviceName: device.deviceName })
          } else if (behavior === 'denied') {
            resolve({ status: 'denied', deviceName: device.deviceName })
          } else {
            reject(new Error('dial failed: node unreachable'))
          }
        }, dialLatencyMs),
      )
    })
  })
  context.commands.register('file-transfer.disconnect-peer', (args: any) => {
    const nodeId = args?.nodeId
    if (typeof nodeId === 'string') emitConnection(nodeId, false)
    return Promise.resolve(true)
  })

  // ==================== 首连确认演示（ticket 04，种子来自插件扩展字段） ====================

  const consentSeed = peerSeed?.consent
  if (consentSeed) {
    // 配对名单种子写入插件存储（useConsent 经 storage.get('paired_devices')
    // 读取做迁移规则匹配）；异步写入不阻塞其余 mock 接线
    void context.storage.set('paired_devices', [...consentSeed.pairedDevices])

    // 应答命令 handler：记录并返回命中（真实宿主由 WASM 代理转发宿主闸门）
    context.commands.register('file-transfer.respond-consent', (args: any) => {
      pushLog(
        'info',
        pluginId,
        `respond-consent (mock): ${args?.requestId} accepted=${args?.accepted}`,
      )
      return { hit: true }
    })

    // 延迟推送首连确认事件，覆盖自动互信与弹窗两路演示
    for (const { delayMs, request } of consentSeed.requests) {
      timers.push(
        setTimeout(() => {
          emitDevEvent('plugin:file-transfer:consent-requested', { ...request })
        }, delayMs),
      )
    }
  }

  // ==================== 可信对端演示（ticket 05，种子来自插件扩展字段） ====================

  // 异形种子回退空列表；撤销从 mock 数组摘除，重进设置页可见最新列表与空态
  const trustedPeers: MockTrustedSeed[] = Array.isArray(peerSeed?.trusted)
    ? (peerSeed!.trusted as MockTrustedSeed[]).map((p) => ({ ...p }))
    : []

  context.commands.register('file-transfer.list-trusted', () =>
    trustedPeers.map((p) => ({ ...p })),
  )
  context.commands.register('file-transfer.revoke-trusted', (args: any) => {
    const nodeId: string = args?.nodeId ?? ''
    const idx = trustedPeers.findIndex((p) => p.nodeId === nodeId)
    if (idx >= 0) trustedPeers.splice(idx, 1)
    pushLog('info', pluginId, `revoke-trusted (mock): ${nodeId} existed=${idx >= 0}`)
    return idx >= 0
  })

  // ==================== 远端浏览（useRemoteFs 契约，种子来自 transfer.remoteFs） ====================

  const remoteRoots = mock?.transfer?.remoteFs?.roots.map((r) => ({ ...r })) ?? []
  const remoteFiles = mock?.transfer?.remoteFs?.files ?? {}
  context.commands.register('file-transfer.list-remote', (args: any) => {
    if (!args?.dirId && !args?.path) {
      return { roots: remoteRoots.map((r) => ({ ...r })) }
    }
    const key = `${args?.dirId ?? ''}::${args?.path ?? ''}`
    const entries =
      remoteFiles[key] ?? (args?.path === '' ? [] : (remoteFiles[`${args?.dirId ?? ''}::`] ?? []))
    return { entries: entries.map((e) => ({ ...e })) }
  })

  // ==================== 设置域（useSettings 契约：宿主 wire DTO 形状，种子来自 transfer.settings） ====================
  // 形状对齐宿主 get_settings 返回值（mapWireSettings 消费 snake_case）；
  // 种子缺省回退协议缺省值（policy ask / 超时 60s / 空下载目录）

  const localRoots: MockSharedDir[] =
    mock?.transfer?.settings?.roots?.map((r) => ({ ...r })) ?? []
  let policyMode: string = mock?.transfer?.settings?.policyMode ?? 'ask'
  let askTimeoutSec: number = mock?.transfer?.settings?.askTimeoutSec ?? 60
  const downloadDir: string = mock?.transfer?.settings?.downloadDir ?? ''

  context.commands.register('file-transfer.get-settings', () => ({
    roots: localRoots.map((r) => ({ ...r })),
    policy_mode: policyMode,
    ask_timeout_sec: askTimeoutSec,
    download_dir: downloadDir,
  }))
  context.commands.register('file-transfer.set-settings', (args: any) => {
    if (typeof args?.receivingPolicy === 'string') {
      policyMode = args.receivingPolicy === 'accept'
        ? 'always_accept'
        : args.receivingPolicy === 'reject'
          ? 'always_deny'
          : 'ask'
    }
    if (typeof args?.approvalTimeoutSec === 'number') {
      askTimeoutSec = Math.min(Math.max(Math.round(args.approvalTimeoutSec), 10), 600)
    }
    return { ok: true }
  })
  // 添加共享目录：模拟 SAF 目录树选择器授权成功（追加条目，同名幂等）；
  // 真实取消路径由宿主选择器决定，mock 直接返回 ok 驱动「添加 → 列表刷新」全链演示
  let safSeq = 0
  context.commands.register('file-transfer.mount-local', () => {
    safSeq += 1
    const entry: MockSharedDir = {
      id: `root-saf-${safSeq}`,
      name: `SDCARD${safSeq > 1 ? safSeq : ''}`,
      tree_uri: `content://com.android.externalstorage.documents/tree/primary%3ADocuments-${safSeq}`,
    }
    if (!localRoots.some((r) => r.tree_uri === entry.tree_uri)) {
      localRoots.push(entry)
    }
    return { ok: true }
  })
  context.commands.register('file-transfer.update-roots', (args: any) => {
    const removeId: string | undefined = typeof args?.remove === 'string' ? args.remove : undefined
    if (!removeId) return { removed: false }
    const target = localRoots.find((r) => r.id === removeId)
    if (!target || target.builtin) return { removed: false }
    const idx = localRoots.indexOf(target)
    localRoots.splice(idx, 1)
    return { removed: true }
  })

  // ==================== 初始与延迟补发 ====================

  // 初始推送（工具箱入口等早订阅者）+ WS 控制面在线（顶栏 pill / connOnline）
  pushPeerSnapshot()
  emitDevEvent('device-connected', {
    device_id: activePeerId || devices[0]?.nodeId || '',
    device_name: devices[0]?.deviceName ?? '',
  })
  emitDevEvent('ws_paired', {})

  // 延迟补发：视图订阅晚于激活时靠它追平（与既有 mock 的延迟富化策略一致）
  timers.push(setTimeout(pushPeerSnapshot, 600))
}

/** 清理模拟定时器（插件停用时调用；命令 handler 随 context disposables 摘除） */
export function disposeFileTransferMock(): void {
  while (timers.length) clearTimeout(timers.pop())
}
