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
 *
 * 传输域种子（transfer 子域，SDK TransferDevMock 协议）：任务列表（8 态覆盖）、
 * 远端共享根与目录树、本机共享设置——原 dev-shell 内置业务数据已全部迁入此处。
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

/** 扩展了 trusted 字段的 peer 种子 + 传输域种子（多余字段对 SDK 协议向后兼容） */
type DevMockWithExtensions = PluginDevMock & {
  peer?: NonNullable<PluginDevMock['peer']> & { trusted?: TrustedDevSeed[] }
}

export const NODE_XIAOMI = 'f3a91c07e5d24b18a7c60f12d94b8e55'
export const NODE_PIXEL = '8b02d641c9ae4f77b3e15a90dd276c84'

const devMock: DevMockWithExtensions = {
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
  // ==================== 传输域种子（SDK TransferDevMock 协议） ====================
  transfer: {
    // 任务快照：覆盖全部 8 态，驱动四色体系展示；paused 条目保留进度与排队（0%）区分
    tasks: [
      {
        id: 'mock-task-1',
        direction: 'download',
        remotePath: 'DCIM/VID_20240801_1820.mp4',
        size: 89244416,
        offset: 41933507, // 47%
        state: 'transferring',
      },
      {
        id: 'mock-task-2',
        direction: 'upload',
        remotePath: '工作文档/产品需求文档_v3.docx',
        localPath: 'C:\\workspace\\产品需求文档_v3.docx',
        size: 248320,
        offset: 248320,
        state: 'completed',
      },
      {
        id: 'mock-task-3',
        direction: 'download',
        remotePath: '2024年度旅行相册.zip',
        size: 2470476800,
        offset: 864667000, // 35%：暂停任务保留已下载进度
        state: 'paused',
      },
      {
        id: 'mock-task-4',
        direction: 'upload',
        remotePath: 'IMG_20240802_0815.jpg',
        localPath: 'D:\\photos\\IMG_20240802_0815.jpg',
        size: 5124300,
        offset: 1024860,
        state: 'transferring',
      },
      {
        id: 'mock-task-5',
        direction: 'download',
        remotePath: '4K测试视频_8分钟.mp4',
        size: 1258291200,
        offset: 0,
        state: 'queued',
      },
      {
        id: 'mock-task-6',
        direction: 'upload',
        remotePath: '毕业设计答辩.pptx',
        localPath: 'D:\\slides\\毕业设计答辩.pptx',
        size: 18677760,
        offset: 0,
        state: 'failed',
        reason: 'duplicate-name',
      },
      {
        id: 'mock-task-7',
        direction: 'download',
        remotePath: '会议录音_产品周会.mp3',
        size: 12695376,
        offset: 12695376,
        state: 'completed',
      },
      {
        id: 'mock-task-8',
        direction: 'upload',
        remotePath: 'Backup_2024-08.tar.gz',
        localPath: 'E:\\backup\\Backup_2024-08.tar.gz',
        size: 4127191040,
        offset: 0,
        state: 'rejected',
        reason: 'duplicate-name',
      },
    ],
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
    // 本机共享设置（roots 为宿主 RootItem DTO：按 name 展示、按 id 寻址移除）
    settings: {
      roots: [
        { id: 'mock-root-1', name: 'C:\\Users\\binblink\\Desktop\\共享文件夹' },
        { id: 'mock-root-2', name: 'E:\\媒体库\\相机导入' },
      ],
      downloadDir: 'C:\\Users\\binblink\\Downloads\\BedCode',
      concurrency: 3,
    },
  },
}

export default devMock
