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
 * - consent 种子：两条待确认首连请求，驱动确认弹窗与状态栏计数两路演示
 * - trusted 种子：三条可信对端（含一条无名短指纹兑底），驱动设置分区
 *   列表展示与两步撤销全流程演示
 */
import type { PluginDevMock } from '@binblink/plugin-sdk-desktop'

/**
 * 可信对端演示种子（ticket 05）：SDK PluginDevMock 协议的本地扩展字段
 *
 * dev-shell mock/file-transfer.ts 消费：list-trusted 返回副本、
 * revoke-trusted 从数组摘除（可演示撤销全流程）。真实宿主忽略。
 */
export interface TrustedDevSeed {
  nodeId: string
  displayName: string | null
  fingerprintShort: string
  /** RFC3339 加入时间 */
  addedAt: string
}

/** 扩展了 trusted 字段的 peer 种子（多余字段对 SDK 协议向后兼容） */
type PeerWithTrusted = NonNullable<PluginDevMock['peer']> & { trusted?: TrustedDevSeed[] }
type PeerDevMockWithTrusted = Omit<PluginDevMock, 'peer'> & { peer?: PeerWithTrusted }

export const NODE_XIAOMI = 'f3a91c07e5d24b18a7c60f12d94b8e55'
export const NODE_PIXEL = '8b02d641c9ae4f77b3e15a90dd276c84'

const peerDevMock: PeerDevMockWithTrusted = {
  peer: {
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
    // 首连确认演示：第一条立即弹窗（有名设备），第二条 2s 后入队（无名设备，
    // 短指纹兑底文案）——可同时驱动确认弹窗与状态栏「{n} 台设备等待确认」计数
    consent: [
      {
        requestId: 'mock-consent-1',
        nodeId: 'e70a4c92d85f41b6a0d3c97f52e1b834',
        fingerprintShort: 'e70a4c92',
        deviceName: 'iPad Pro',
      },
      {
        requestId: 'mock-consent-2',
        nodeId: '44f19b7ce2a84d6591b07ad3c58ef206',
        fingerprintShort: '44f19b7c',
        deviceName: null,
      },
    ],
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
        nodeId: 'e70a4c92d85f41b6a0d3c97f52e1b834',
        displayName: 'iPad Pro',
        fingerprintShort: 'e70a4c92',
        addedAt: '2026-08-02T18:47:00+08:00',
      },
      {
        nodeId: '9a3c57e1b04d42f89d6ce21ba75f0d38',
        displayName: null,
        fingerprintShort: '9a3c57e1',
        addedAt: '2026-08-20T11:05:00+08:00',
      },
    ],
  },
}

export default peerDevMock
