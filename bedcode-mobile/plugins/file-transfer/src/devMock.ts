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
import type { PeerDevMock, PluginDevMock } from '@binblink/plugin-sdk-mobile'

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

const peerDevMock: PeerDevMockWithTrusted = {
  devices: [
    {
      nodeId: NODE_XIAOMI,
      deviceName: '小米 14 Pro',
      addr: '192.168.1.108:47821',
      fileTransfer: true,
    },
    {
      nodeId: NODE_PIXEL,
      deviceName: 'Pixel 9',
      addr: '192.168.1.132:51044',
      fileTransfer: true,
    },
    {
      nodeId: '51c8aa93e07b4d2f96d3b1c45f8ea720',
      deviceName: '客厅电视 BedBox',
      addr: '192.168.1.120:47613',
      fileTransfer: true,
    },
    {
      nodeId: '9d64b2f08c1e4735ae02d7b6cc4910e3',
      deviceName: 'Old Laptop',
      addr: '192.168.1.77:47613',
      fileTransfer: true,
    },
    {
      nodeId: 'c47d19f2ab354e6180d92b7ce30a5f16',
      deviceName: 'HomeNAS',
      addr: '192.168.1.2:47613',
      fileTransfer: false,
    },
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
