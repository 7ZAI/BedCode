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
import type { PluginContext, PluginDevMock } from '../../../src/types'
import { pushLog } from '../registry'

/** 设备种子条目（插件 devMock 导出的本地扩展字段：mdns:found 载荷形状 + 拨号行为） */
interface MockDeviceSeed {
  found: {
    instanceName: string
    addresses: string[]
    port: number
    txtRecords: { id: string; name?: string; ver?: string; cap?: string }
  }
  dialBehavior?: 'connected' | 'denied' | 'unreachable'
}

/** 对等域种子结构（插件 devMock.peer 的通用形状，dev-shell 只按需取字段） */
interface MockPeerSeed {
  connectedNodeIds?: string[]
  activeNodeId?: string
  dialLatencyMs?: number
}

/** 传输域种子结构（插件 devMock.transfer 的通用形状） */
interface MockTransferSeed {
  remoteFs?: {
    roots?: Array<{ id: string; name: string }>
    files?: Record<string, Array<{ name: string; size: number; mtime: number; isDir: boolean }>>
  }
  settings?: {
    roots?: MockSharedDir[]
    policyMode?: string
    askTimeoutSec?: number
    downloadDir?: string
  }
  /** 任务快照种子（插件本地扩展字段；wire camelCase 形状，useTasks mapWire* 消费） */
  tasks?: {
    queue: Array<Record<string, unknown>>
    receiving: Array<Record<string, unknown>>
    history: Array<Record<string, unknown>>
  }
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
  const peerSeed = mock?.peer as
    | (MockPeerSeed & { consent?: MockConsentSeed; trusted?: unknown; deviceSeeds?: MockDeviceSeed[] })
    | undefined
  const deviceSeeds: MockDeviceSeed[] = peerSeed?.deviceSeeds ?? []
  /** 种子派生视图：nodeId → 种子（拨号行为/能力位判定用） */
  const seedByNode = new Map(deviceSeeds.map((d) => [d.found.txtRecords.id, d]))
  const connectedNodes = new Set<string>(peerSeed?.connectedNodeIds ?? [])
  let activePeerId = peerSeed?.activeNodeId ?? ''
  const dialLatencyMs = peerSeed?.dialLatencyMs ?? 800

  // ==================== 对等域事件推送 ====================

  /** 连接态增量：维护已连接集合并推 connection-changed（{ nodeId, connected } 契约） */
  function emitConnection(nodeId: string, connected: boolean): void {
    const seed = seedByNode.get(nodeId)
    if (connected) {
      if (!seed || connectedNodes.has(nodeId)) return
      connectedNodes.add(nodeId)
    } else {
      if (!connectedNodes.has(nodeId)) return
      connectedNodes.delete(nodeId)
    }
    emitDevEvent('plugin:file-transfer:connection-changed', {
      nodeId,
      connected,
      deviceName: seed?.found.txtRecords.name ?? null,
    })
  }

  /** 发现推送：逐台延迟发 mdns-found（自建缓存版 wire；晚订阅靠延迟窗口追平） */
  function pushDeviceDiscovery(): void {
    deviceSeeds.forEach((seed, i) => {
      timers.push(
        setTimeout(() => {
          emitDevEvent('plugin:file-transfer:mdns-found', { ...seed.found })
        }, 400 + i * 300),
      )
    })
  }

  /** 连接态全量补发（dev-shell 事件总线不重放历史，晚订阅者靠它追平） */
  function pushPeerSnapshot(): void {
    for (const nodeId of connectedNodes) emitConnection(nodeId, true)
  }

  // ==================== 对等域命令 handler ====================

  // 设备缓存自持（Phase 3 步骤 1）：快照存取 + 发现事件延迟推送。
  // get-device-snapshot 从种子派生快照（wire 形状与宿主一致）：插件视图挂载
  // 晚于激活事件时，设备仍能由「最近可见」快照恢复首屏展示（真实宿主行为同源）
  context.commands.register('file-transfer.get-device-snapshot', () => ({
    devices: deviceSeeds.map((d, i) => ({
      nodeId: d.found.txtRecords.id,
      deviceName: d.found.txtRecords.name ?? '',
      addr: d.found.addresses[0] ?? '',
      port: d.found.port,
      capabilitiesHex: d.found.txtRecords.cap ?? '0',
      instanceName: d.found.instanceName,
      lastSeenMs: Date.now() - i * 8000,
    })),
  }))
  context.commands.register('file-transfer.save-device-snapshot', () => ({ ok: true }))
  context.commands.register('file-transfer.set-active-peer', (args: any) => {
    const id = args?.peerId
    if (typeof id === 'string' && (!id || seedByNode.has(id))) {
      activePeerId = id
    }
    return { activePeerId }
  })
  // 对等拨号：入参携带显式 endpoint；按种子 dialBehavior 返回终态（错误以
  // rejected promise 携带字样供行内文案分流）；connected 时推连接态事件
  context.commands.register('file-transfer.dial-peer', (args: any) => {
    const nodeId: string = args?.endpoint?.nodeId ?? args?.nodeId ?? ''
    const seed = seedByNode.get(nodeId)
    const capable = Number.parseInt(seed?.found.txtRecords.cap ?? '0', 16) % 2 === 1
    if (!seed || !capable || connectedNodes.has(nodeId)) {
      return Promise.reject(new Error(`dial endpoint failed: peer unreachable (${nodeId})`))
    }
    const behavior = seed.dialBehavior ?? 'unreachable'
    return new Promise((resolve, reject) => {
      timers.push(
        setTimeout(() => {
          if (behavior === 'connected') {
            emitConnection(nodeId, true)
            resolve({ status: 'connected' })
          } else if (behavior === 'denied') {
            reject(new Error('dial endpoint failed: peer denied'))
          } else {
            reject(new Error('dial endpoint failed: peer unreachable'))
          }
        }, dialLatencyMs),
      )
    })
  })
  context.commands.register('file-transfer.disconnect-peer', (args: any) => {
    const nodeId = args?.nodeId
    if (typeof nodeId === 'string') emitConnection(nodeId, false)
    return Promise.resolve({ existed: true })
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

  const transfer = mock?.transfer as MockTransferSeed | undefined
  const remoteRoots = transfer?.remoteFs?.roots?.map((r) => ({ ...r })) ?? []
  const remoteFiles = transfer?.remoteFs?.files ?? {}
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
    transfer?.settings?.roots?.map((r) => ({ ...r })) ?? []
  let policyMode: string = transfer?.settings?.policyMode ?? 'ask'
  let askTimeoutSec: number = transfer?.settings?.askTimeoutSec ?? 60
  const downloadDir: string = transfer?.settings?.downloadDir ?? ''

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
  // 添加共享目录（Phase 3 自持版契约）：模拟 SAF 目录树选择器授权成功，
  // 返回注册表条目 { id, name, treeUri }；真实取消路径由宿主选择器决定
  let safSeq = localRoots.filter((r) => !r.builtin).length
  context.commands.register('file-transfer.mount-local', () => {
    safSeq += 1
    const uri = `content://com.android.externalstorage.documents/tree/primary%3ADocuments-${safSeq}`
    const entry: MockSharedDir = {
      id: `root-saf-${safSeq}`,
      name: `SDCARD${safSeq > 1 ? safSeq : ''}`,
      tree_uri: uri,
    }
    if (!localRoots.some((r) => r.tree_uri === entry.tree_uri)) {
      localRoots.push(entry)
    }
    return { id: entry.id, name: entry.name, treeUri: entry.tree_uri }
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

  // ==================== 传输任务快照（useTasks 契约：list-* 命令 + *-changed 事件） ====================
  // 种子来自插件 devMock.transfer.tasks（业务数据归插件工程，此处仅命令骨架接线），
  // 缺省回退空数组 —— 插件未导出任务种子时传输 tab 演示空态。
  // wire 形状与宿主 PeerTransferDto 对齐（camelCase），useTasks 映射层原样消费。

  const taskSeed = transfer?.tasks
  const taskQueue = taskSeed?.queue?.map((t) => ({ ...t })) ?? []
  const receivingSeed = taskSeed?.receiving?.map((t) => ({ ...t })) ?? []
  const historySeed = taskSeed?.history?.map((t) => ({ ...t })) ?? []

  context.commands.register('file-transfer.list-tasks', () => taskQueue.map((t) => ({ ...t })))
  context.commands.register('file-transfer.list-batches', () => [])
  context.commands.register('file-transfer.list-receiving', () => receivingSeed.map((t) => ({ ...t })))
  context.commands.register('file-transfer.list-history', () => historySeed.map((t) => ({ ...t })))

  // 快照整表事件补发（useTasks start() 订阅整表替换；延迟推送模拟宿主激活后首报，
  // 与初始命令同步幂等，double-push 无害）
  if (taskQueue.length || receivingSeed.length || historySeed.length) {
    timers.push(
      setTimeout(() => {
        emitDevEvent('plugin:file-transfer:tasks-changed', taskQueue.map((t) => ({ ...t })))
        emitDevEvent('plugin:file-transfer:receiving-changed', receivingSeed.map((t) => ({ ...t })))
        emitDevEvent('plugin:file-transfer:history-changed', historySeed.map((t) => ({ ...t })))
      }, 1500),
    )
  }

  // ==================== 初始与延迟补发 ====================

  // 发现事件延迟推送（自建缓存 wire）+ 连接态补发 + WS 控制面在线
  pushDeviceDiscovery()
  timers.push(
    setTimeout(() => {
      pushPeerSnapshot()
      emitDevEvent('device-connected', {
        device_id: activePeerId || deviceSeeds[0]?.found.txtRecords.id || '',
        device_name: deviceSeeds[0]?.found.txtRecords.name ?? '',
      })
      emitDevEvent('ws_paired', {})
    }, 600),
  )

  // 激活事件早于插件视图挂载（dev-shell 事件总线不重放历史，晚订阅者会漏收）：
  // 在常见挂载窗口后再整表重放两次（6s / 14s），幂等合并，覆盖手动/截图两种节奏。
  // 纯通用接线：不新增任何业务数据，只是把种子已有状态重新广播一遍。
  function replayPulse(): void {
    deviceSeeds.forEach((seed) => {
      emitDevEvent('plugin:file-transfer:mdns-found', { ...seed.found })
    })
    for (const nodeId of connectedNodes) {
      const seed = seedByNode.get(nodeId)
      emitDevEvent('plugin:file-transfer:connection-changed', {
        nodeId,
        connected: true,
        deviceName: seed?.found.txtRecords.name ?? null,
      })
    }
    emitDevEvent('device-connected', {
      device_id: activePeerId || deviceSeeds[0]?.found.txtRecords.id || '',
      device_name: deviceSeeds[0]?.found.txtRecords.name ?? '',
    })
    emitDevEvent('ws_paired', {})
  }
  timers.push(setTimeout(replayPulse, 6000), setTimeout(replayPulse, 14000))
}

/** 清理模拟定时器（插件停用时调用；命令 handler 随 context disposables 摘除） */
export function disposeFileTransferMock(): void {
  while (timers.length) clearTimeout(timers.pop())
}
