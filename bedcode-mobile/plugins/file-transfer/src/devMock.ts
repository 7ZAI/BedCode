/**
 * File Transfer 插件 dev-shell 领域种子数据（SDK PluginDevMock 协议）
 *
 * 仅 dev-shell 浏览器演示消费（loader 按 pluginId 注册，mock 命令实现
 * 消费种子返回演示值）；真实宿主忽略该导出，无需条件编译。
 *
 * 对等子数据覆盖附近设备面板的全部演示态：
 * - 小米 14 Pro：初始已连接且为活跃对端
 * - Pixel 9：未连接，拨号延迟后成功（可演示握手过程与设为当前）
 * - 客厅电视 BedBox：拨号被拒（denied 行内错误文案）
 * - Old Laptop：拨号不可达（unreachable 行内错误文案）
 * - HomeNAS：无文件传输能力（可见但不可连接）
 * - consent 种子：配对设备自动互信 toast / 陌生设备弹窗 / 无名短指纹兑底三路演示
 * - trusted 种子：三条可信对端（含一条无名短指纹兑底），驱动设置分区列表展示
 *   与两步撤销全流程演示
 */
import type { PluginDevMock } from '@binblink/bedcode-plugin-sdk-mobile'

/** 设备种子（mdns:found 载荷形状 + 拨号行为标注；插件自有类型，SDK 不收录） */
export interface PeerDeviceSeed {
  found: {
    instanceName: string
    addresses: string[]
    port: number
    txtRecords: { id: string; name: string; ver: string; cap: string }
  }
  dialBehavior: 'connected' | 'denied' | 'unreachable'
}

/** 对等域种子（插件自有类型；dev-shell mock 按 deviceSeeds 逐台推发现/拨号） */
export interface PeerDevMock {
  deviceSeeds: PeerDeviceSeed[]
  connectedNodeIds?: string[]
  activeNodeId?: string
  dialBehavior?: Record<string, 'connected' | 'denied' | 'unreachable'>
  dialLatencyMs?: number
}

/**
 * consent 演示种子（ticket 04）：SDK PluginDevMock 协议的本地扩展字段
 *
 * dev-shell mock/file-transfer.ts 消费：pairedDevices 写入插件存储驱动迁移
 * 规则命中；requests 按 delayMs 延迟推送 consent-requested 事件。真实宿主忽略。
 */
export interface ConsentDevSeed {
  /** 终端配对名单种子（设备名数组；写入插件存储 paired_devices 键） */
  pairedDevices: string[]
  /** 激活后延迟推送的首连确认请求（覆盖弹窗与自动互信两路演示） */
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

/**
 * 可信对端演示种子（ticket 05）：SDK PluginDevMock 协议的本地扩展字段
 *
 * dev-shell mock/file-transfer.ts 消费：list-trusted 返回副本、
 * revoke-trusted 从数组摘除（可演示撤销全流程）。真实宿主忽略。
 */
interface TrustedDevSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  /** RFC3339 加入时间 */
  addedAt: string
}

type PeerDevMockWithTrusted = Omit<PeerDevMock, 'consent'> & {
  consent?: ConsentDevSeed
  trusted?: TrustedDevSeed[]
}

/** SDK PluginDevMock 协议包装（loader 按 .peer / .transfer 读取；桌面同构） */
type DevMockWithPeer = PluginDevMock & { peer?: PeerDevMockWithTrusted }

export const NODE_XIAOMI = 'f3a91c07e5d24b18a7c60f12d94b8e55'
export const NODE_PIXEL = '8b02d641c9ae4f77b3e15a90dd276c84'

/** 单台设备种子（mdns:found 载荷形状 + 拨号行为标注；桌面同构） */
function deviceSeed(nodeId: string, name: string, ip: string, port: number, capable = true) {
  const short = nodeId.slice(0, 8)
  return {
    found: {
      instanceName: `bedcode-peer-${short}._bedcode-peer._tcp.local.`,
      addresses: [ip],
      port,
      txtRecords: { id: nodeId, name, ver: '1', cap: capable ? '1' : '0' },
    },
    dialBehavior: 'connected' as 'connected' | 'denied' | 'unreachable',
  }
}

const peerDevMock: PeerDevMockWithTrusted = {
  // mdns:found 载荷形状种子（dev-shell 逐台延迟推送）；SDK 协议 devices 字段
  // 类型未收录该形状，以 unknown 断言注入（dev-shell mock 消费本地扩展字段）
  deviceSeeds: [
    { ...deviceSeed(NODE_XIAOMI, '小米 14 Pro', '192.168.1.108', 47821), dialBehavior: 'connected' as const },
    { ...deviceSeed(NODE_PIXEL, 'Pixel 9', '192.168.1.132', 51044), dialBehavior: 'connected' as const },
    { ...deviceSeed('51c8aa93e07b4d2f96d3b1c45f8ea720', '客厅电视 BedBox', '192.168.1.120', 47613), dialBehavior: 'denied' as const },
    { ...deviceSeed('9d64b2f08c1e4735ae02d7b6cc4910e3', 'Old Laptop', '192.168.1.77', 47613), dialBehavior: 'unreachable' as const },
    { ...deviceSeed('c47d19f2ab354e6180d92b7ce30a5f16', 'HomeNAS', '192.168.1.2', 47613, false) },
  ],
  connectedNodeIds: [NODE_XIAOMI],
  activeNodeId: NODE_XIAOMI,
  dialBehavior: {
    [NODE_PIXEL]: 'connected',
    '51c8aa93e07b4d2f96d3b1c45f8ea720': 'denied',
    '9d64b2f08c1e4735ae02d7b6cc4910e3': 'unreachable',
  },
  dialLatencyMs: 800,
  consent: {
    // 小米 14 Pro 为终端配对设备：consent 请求命中名单 → 静默自动互信 + toast
    pairedDevices: ['小米 14 Pro'],
    requests: [
      {
        // 自动互信路径演示：配对设备在激活后约 2s 发起连接
        delayMs: 2000,
        request: {
          requestId: 'demo-consent-auto-trust',
          nodeId: NODE_XIAOMI,
          fingerprintShort: NODE_XIAOMI.slice(0, 8),
          deviceName: '小米 14 Pro',
        },
      },
      {
        // 弹窗路径演示：陌生具名设备在自动互信后约 4s 发起连接
        delayMs: 6000,
        request: {
          requestId: 'demo-consent-dialog',
          nodeId: '44f19b7c8d23e5a16094b7c2f1d3e5a7',
          fingerprintShort: '44f19b7c',
          deviceName: 'iPad Pro',
        },
      },
      {
        // 无名兜底文案演示：短指纹作为展示名 + 核对提示
        delayMs: 14000,
        request: {
          requestId: 'demo-consent-nameless',
          nodeId: '9a3c57e1b04d42f89d6ce21ba75f0d38',
          deviceName: null,
        },
      },
    ],
  },
  // 可信对端种子：小米 14 Pro / iPad Pro 具名 + 一条无名条目演示短指纹兑底；
  // 撤销后从 mock 数组摘除，重进设置页可见空态
  trusted: [
    {
      nodeId: NODE_XIAOMI,
      displayName: '小米 14 Pro',
      fingerprintShort: NODE_XIAOMI.slice(0, 8),
      addedAt: '2026-07-12T09:24:00+08:00',
    },
    {
      nodeId: '44f19b7c8d23e5a16094b7c2f1d3e5a7',
      displayName: 'iPad Pro',
      fingerprintShort: '44f19b7c',
      addedAt: '2026-08-02T18:47:00+08:00',
    },
    {
      nodeId: '9a3c57e1b04d42f89d6ce21ba75f0d38',
      displayName: null,
      fingerprintShort: '9a3c57e1',
      addedAt: '2026-08-20T11:05:00+08:00',
    },
  ],
}

// SDK PluginDevMock 协议要求 { peer } 包装（loader 按种子子域判断注入；
// 此前导出扁平 PeerDevMock 导致 dev-shell 种子永远读不到，面板/consent 全空）
const devMock: DevMockWithPeer = {
  peer: peerDevMock,
  // ==================== 传输域种子（SDK TransferDevMock 协议） ====================
  transfer: {
    // 远端共享根（对端设备侧演示目录；dirId + 根内相对路径寻址，契约见 useRemoteFs）
    remoteFs: {
      roots: [
        { id: 'root-dcim', name: 'DCIM' },
        { id: 'root-download', name: 'Download' },
        { id: 'root-weixin', name: '微信文件' },
      ],
      files: {
        'root-dcim::': [
          { name: 'Camera', size: 0, mtime: 1754688000, isDir: true },
          { name: 'Screenshots', size: 0, mtime: 1754662000, isDir: true },
          { name: 'IMG_20240801_1932.jpg', size: 4869382, mtime: 1754664000, isDir: false },
          { name: 'VID_20240801_1820.mp4', size: 89244416, mtime: 1754665000, isDir: false },
        ],
        'root-dcim::Camera': [
          { name: 'IMG_20240801_1800.jpg', size: 4123400, mtime: 1754664000, isDir: false },
          { name: 'IMG_20240801_1815.jpg', size: 3891100, mtime: 1754664600, isDir: false },
        ],
        'root-download::': [
          { name: 'BedCode-2.0.0.apk', size: 68_000_000, mtime: 1754560000, isDir: false },
          { name: 'Ubuntu-24.04.iso', size: 4_720_000_000, mtime: 1754550000, isDir: false },
        ],
        'root-weixin::': [
          { name: '产品需求文档_v3.docx', size: 248320, mtime: 1754577000, isDir: false },
          { name: '会议录音_产品周会.mp3', size: 12695376, mtime: 1754520000, isDir: false },
        ],
      },
    },
    // 任务快照种子（本地扩展字段；wire camelCase 形状，useTasks mapWire*
    // 消费，dev-shell mock 按此实现 list-tasks / list-receiving / list-history
    // 命令骨架 + 快照事件补发）。覆盖传输 tab 四象限：发送中（进度+速率）、
    // 发送失败（原因块 + 重试）、接收中、历史（完成/失败/取消，相对时间）。
    // 注意：wire status 非 runner 态只有 running（→transferring）与终态，
    // 没有 queued（TASK_STATE_KEYS 无该键，注入会渲染 undefined）
    tasks: {
      // 发送方向任务列表（file-transfer.list-tasks）
      queue: [
        {
          batchId: 'demo-send-img-01',
          direction: 'send',
          nodeId: NODE_XIAOMI,
          peerName: '小米 14 Pro',
          files: [
            { path: '/DCIM/IMG_20240801_1932.jpg', size: 4869382 },
            { path: '/DCIM/IMG_20240801_1940.jpg', size: 4123400 },
            { path: '/DCIM/IMG_20240801_1955.jpg', size: 3891100 },
            { path: '/DCIM/IMG_20240801_2001.jpg', size: 4217800 },
          ],
          totalBytes: 17101682,
          transferredBytes: 4380000,
          rateBps: 2240000,
          status: 'running',
          createdAtMs: 1754686000000,
          updatedAtMs: 1754686000000,
        },
        {
          // 失败条目：rejectReason 命中 taskReason 映射 → 行内原因块 + 重试
          batchId: 'demo-send-doc-02',
          direction: 'send',
          nodeId: '51c8aa93e07b4d2f96d3b1c45f8ea720',
          peerName: '客厅电视 BedBox',
          files: [
            { path: '/微信文件/产品需求文档_v3.docx', size: 248320 },
            { path: '/微信文件/产品需求文档_v3_修订.docx', size: 296960 },
          ],
          totalBytes: 545280,
          transferredBytes: 198000,
          rateBps: 0,
          status: 'failed',
          rejectReason: 'no-roots',
          createdAtMs: 1754680000000,
          updatedAtMs: 1754681000000,
        },
      ],
      // 正在接收列表（file-transfer.list-receiving）
      receiving: [
        {
          batchId: 'demo-recv-video-01',
          nodeId: NODE_XIAOMI,
          peerName: '小米 14 Pro',
          files: [{ path: '/DCIM/VID_20240801_1820.mp4', size: 89244416 }],
          totalBytes: 89244416,
          transferredBytes: 31800000,
          rateBps: 3400000,
          status: 'running',
          detail: null,
          createdAtMs: 1754685000000,
          updatedAtMs: 1754685000000,
        },
      ],
      // 历史列表（file-transfer.list-history；status 必须为终态）
      history: [
        {
          // 完成条目带 localPath：驱动历史卡「打开所在文件夹」演示链路
          batchId: 'demo-hist-completed-01',
          direction: 'receive',
          nodeId: NODE_XIAOMI,
          peerName: '小米 14 Pro',
          files: [{ path: '/微信文件/微信图片_20240801_201234.jpg', size: 2433600 }],
          totalBytes: 2433600,
          status: 'completed',
          localPath: '/storage/emulated/0/Download/微信图片_20240801_201234.jpg',
          createdAtMs: Date.now() - 34 * 60000,
          updatedAtMs: Date.now() - 33 * 60000,
        },
        {
          batchId: 'demo-hist-failed-01',
          direction: 'send',
          nodeId: '51c8aa93e07b4d2f96d3b1c45f8ea720',
          peerName: '客厅电视 BedBox',
          files: [{ path: '/Download/Backup_20240801.zip', size: 4820000000 }],
          totalBytes: 4820000000,
          status: 'failed',
          rejectReason: 'user-rejected',
          createdAtMs: Date.now() - 10 * 60000,
          updatedAtMs: Date.now() - 9 * 60000,
        },
        {
          batchId: 'demo-hist-cancelled-01',
          direction: 'send',
          nodeId: NODE_PIXEL,
          peerName: 'Pixel 9',
          files: [{ path: '/Download/安卓备份_2024.tar.gz', size: 268435456 }],
          totalBytes: 268435456,
          status: 'cancelled',
          createdAtMs: Date.now() - 190 * 60000,
          updatedAtMs: Date.now() - 185 * 60000,
        },
      ],
    },
    // 本机共享设置（roots 为宿主 wire DTO 形状，含 SAF tree_uri）
    settings: {
      roots: [
        {
          id: 'builtin-private-downloads',
          name: 'app 私有下载目录',
          tree_uri: '',
          builtin: true,
        },
      ],
      policyMode: 'ask',
      askTimeoutSec: 60,
      downloadDir: 'MediaStore/Downloads',
    },
  },
}
export default devMock
