/**
 * File Transfer 插件 dev-shell 领域种子数据（SDK PluginDevMock 协议）
 *
 * 仅 dev-shell 浏览器演示消费（loader 按 pluginId 注册，mock 命令实现
 * 消费种子返回演示值）；真实宿主忽略该导出，无需条件编译。
 *
 * Phase 3 自持版种子全部为 wire 形状（与真实事件载荷同构）：
 * - 设备：mdns:found 载荷形状（instanceName/addresses/port/txtRecords），
 *   由 dev-shell mock 逐台延迟推送驱动前端缓存状态机；
 * - 任务：引擎 PeerTransferDto camelCase 形状（batchId/status/files[]/
 *   *Bytes/*Ms），覆盖传输中/终态/interrupted 演示态；
 * - 共享设置/远端文件树：沿用既有契约形状。
 */
import type { FileTransferDevMock, PeerDevMock } from './devMockTypes'

export type { TrustedDevSeed } from './devMockTypes'

export const NODE_XIAOMI = 'f3a91c07e5d24b18a7c60f12d94b8e55'
export const NODE_PIXEL = '8b02d641c9ae4f77b3e15a90dd276c84'

/** 单台设备种子（mdns:found 载荷形状 + 拨号行为标注） */
function deviceSeed(nodeId: string, name: string, ip: string, port: number, capable = true) {
  const short = nodeId.slice(0, 8)
  return {
    found: {
      instanceName: `bedcode-peer-${short}._bedcode-peer._tcp.local.`,
      addresses: [ip],
      port,
      txtRecords: {
        id: nodeId,
        name,
        ver: '1',
        // bit0 = 文件传输；不可传设备置 0
        cap: capable ? '1' : '0',
      },
    },
    /** 拨号演示行为（缺省 unreachable） */
    dialBehavior: 'connected' as 'connected' | 'denied' | 'unreachable',
  }
}

const devMock: FileTransferDevMock = {
  peer: {
    // mdns:found 载荷形状种子（dev-shell 逐台延迟推送）；dialBehavior 按 nodeId 索引
    deviceSeeds: [
      { ...deviceSeed(NODE_XIAOMI, '小米 14 Pro', '192.168.1.108', 47821), dialBehavior: 'connected' as const },
      { ...deviceSeed(NODE_PIXEL, 'Pixel 9', '192.168.1.132', 51044), dialBehavior: 'connected' as const },
      { ...deviceSeed('51c8aa93e07b4d2f96d3b1c45f8ea720', '客厅电视 BedBox', '192.168.1.120', 47613), dialBehavior: 'denied' as const },
      { ...deviceSeed('9d64b2f08c1e4735ae02d7b6cc4910e3', 'Old Laptop', '192.168.1.77', 47613), dialBehavior: 'unreachable' as const },
      { ...deviceSeed('c47d19f2ab354e6180d92b7ce30a5f16', 'HomeNAS', '192.168.1.2', 47613, false) },
    ] as PeerDevMock['deviceSeeds'],
    connectedNodeIds: [NODE_XIAOMI],
    activeNodeId: NODE_XIAOMI,
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
  // ==================== 传输域种子（SDK TransferDevMock 协议） ====================
  transfer: {
    // 任务快照：引擎 PeerTransferDto camelCase wire 形状，覆盖传输中/终态/interrupted
    tasks: [
      {
        batchId: 'mock-task-1',
        nodeId: NODE_XIAOMI,
        peerName: '小米 14 Pro',
        direction: 'receive' as const,
        status: 'running' as const,
        files: [{ path: 'DCIM/VID_20240801_1820.mp4', size: 89244416 }],
        totalBytes: 89244416,
        transferredBytes: 41933507, // 47%
        rateBps: 2_400_000,
        createdAtMs: Date.now() - 600_000,
        updatedAtMs: Date.now() - 1000,
      },
      {
        batchId: 'mock-task-2',
        nodeId: NODE_PIXEL,
        peerName: 'Pixel 9',
        direction: 'send' as const,
        status: 'completed' as const,
        files: [{ path: '工作文档/产品需求文档_v3.docx', size: 248320 }],
        totalBytes: 248320,
        transferredBytes: 248320,
        rateBps: 0,
        detail: null,
        retryMeta: { kind: 'send', paths: ['C:/workspace/产品需求文档_v3.docx'] },
        createdAtMs: Date.now() - 700_000,
        updatedAtMs: Date.now() - 650_000,
      },
      {
        batchId: 'mock-task-6',
        nodeId: NODE_XIAOMI,
        peerName: '小米 14 Pro',
        direction: 'send' as const,
        status: 'failed' as const,
        files: [{ path: '毕业设计答辩.pptx', size: 18677760 }],
        totalBytes: 18677760,
        transferredBytes: 0,
        rateBps: 0,
        detail: 'connection lost mid-transfer',
        rejectReason: null,
        retryMeta: { kind: 'send', paths: ['D:/slides/毕业设计答辩.pptx'] },
        createdAtMs: Date.now() - 300_000,
        updatedAtMs: Date.now() - 240_000,
      },
      {
        batchId: 'mock-task-9',
        nodeId: NODE_PIXEL,
        peerName: 'Pixel 9',
        direction: 'receive' as const,
        status: 'interrupted' as const,
        files: [{ path: 'Backup_2024-08.tar.gz', size: 4127191040 }],
        totalBytes: 4127191040,
        transferredBytes: 1_048_576_000, // 25%
        rateBps: 0,
        detail: null,
        retryMeta: {
          kind: 'pull',
          dirId: 'root-download',
          files: [{ relPath: 'Backup_2024-08.tar.gz', size: 4127191040 }],
        },
        createdAtMs: Date.now() - 900_000,
        updatedAtMs: Date.now() - 850_000,
      },
    ] as unknown[],
    // 远端共享根（对端设备侧演示目录；dirId + 根内相对路径寻址，契约见 useRemoteFs）
    remoteFs: {
      roots: [
        { id: 'root-dcim', name: 'DCIM' },
        { id: 'root-download', name: 'Download' },
        { id: 'root-weixin', name: '微信文件' },
        { id: 'root-docs', name: '工作文档' },
      ],
      files: {
        'root-dcim::': [
          { name: 'Camera', size: 0, mtime: 1754688000, isDir: true },
          { name: 'Screenshots', size: 0, mtime: 1754662000, isDir: true },
          { name: 'IMG_20240801_1932.jpg', size: 4869382, mtime: 1754664000, isDir: false },
          { name: 'IMG_20240802_0815.jpg', size: 5124300, mtime: 1754676000, isDir: false },
          { name: 'VID_20240801_1820.mp4', size: 89244416, mtime: 1754665000, isDir: false },
        ],
        'root-dcim::Camera': [
          { name: 'IMG_20240801_1800.jpg', size: 4123400, mtime: 1754664000, isDir: false },
          { name: 'IMG_20240801_1815.jpg', size: 3891100, mtime: 1754664600, isDir: false },
        ],
        'root-dcim::Screenshots': [
          { name: 'Screenshot_20240802_1015.png', size: 1843200, mtime: 1754700900, isDir: false },
          { name: 'Screenshot_20240802_1432.png', size: 2210400, mtime: 1754716300, isDir: false },
        ],
        'root-download::': [
          { name: 'apk-backup', size: 0, mtime: 1754690000, isDir: true },
          { name: 'BedCode-2.0.0.apk', size: 68_000_000, mtime: 1754560000, isDir: false },
          { name: 'Ubuntu-24.04.iso', size: 4_720_000_000, mtime: 1754550000, isDir: false },
          { name: 'Backup_2024-08.tar.gz', size: 4127191040, mtime: 1754694000, isDir: false },
        ],
        'root-weixin::': [
          { name: '产品需求文档_v3.docx', size: 248320, mtime: 1754577000, isDir: false },
          { name: '销售数据汇总.xlsx', size: 96_000, mtime: 1754570000, isDir: false },
          { name: '会议录音_产品周会.mp3', size: 12695376, mtime: 1754520000, isDir: false },
          { name: '4K测试视频_8分钟.mp4', size: 1258291200, mtime: 1754598000, isDir: false },
          { name: '4K蓝光_星际穿越.mkv', size: 4_100_000_000, mtime: 1754600000, isDir: false },
        ],
        'root-docs::': [
          { name: '产品说明书.pdf', size: 8_600_000, mtime: 1754580000, isDir: false },
          { name: '毕业设计答辩.pptx', size: 18677760, mtime: 1754512000, isDir: false },
          { name: '2024年度旅行相册.zip', size: 2470476800, mtime: 1754628000, isDir: false },
          { name: '系统更新日志.txt', size: 15240, mtime: 1754640000, isDir: false },
          { name: 'main.ts', size: 12_480, mtime: 1754540000, isDir: false },
        ],
      },
    },
    // 本机共享注册表种子（插件自持真源；get-settings 返回 {id,name,path} 条目）
    settings: {
      roots: [
        { id: 'mock-root-1', name: '共享文件夹', path: 'C:\\Users\\binblink\\Desktop\\共享文件夹' },
        { id: 'mock-root-2', name: '相机导入', path: 'E:\\媒体库\\相机导入' },
      ],
      downloadDir: 'C:\\Users\\binblink\\Downloads\\BedCode',
      concurrency: 3,
    },
  },
}

export default devMock
