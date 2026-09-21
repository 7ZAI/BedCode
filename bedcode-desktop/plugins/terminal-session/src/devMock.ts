/**
 * Terminal Session Center 插件 dev-shell 领域种子数据（票 13）
 *
 * SDK `PluginDevMock` 只约定「入口导出 devMock」的通用容器协议
 * （`Record<string, unknown>`），不感知插件领域细节——种子归插件工程持有。
 *
 * 用途有二：
 * 1. dev-shell 领域接线（浏览器中 WASM 后端不可用）——接线本身归 dev-shell
 *    的 mock 模块，本文件只提供数据；
 * 2. 插件视图测试用它驱动 mock PluginContext（`session.config.list` /
 *    `context.session.list()` 的返回值），保证测试数据与演示数据同源。
 *
 * 种子形状与本插件命令面回执一致：配置 = `session.config.list` 的 camelCase
 * 数组；会话 = 宿主 `SessionInfo` 视图子集（camelCase）。
 */
import type { SessionConfigDto, SessionDto } from './composables/useSessionCenter'
import type { ConnectionHistoryEntry } from './composables/useConnectionHistory'
import type {
  LocalNetworkInfo,
  PairedDeviceInfo,
  PairingCodeInfo,
  QrConnectionInfo,
} from './composables/useDeviceCenter'

/** 会话域种子 */
export interface SessionDevSeed {
  /** 配置列表演示数据（`session.config.list` 回执形状） */
  configs: SessionConfigDto[]
  /** 会话列表演示数据（`context.session.list()` 回执形状） */
  sessions: SessionDto[]
  /** 新建配置默认值演示数据（插件存储 `session.formDefaults`） */
  formDefaults: {
    environment: string
    workingDir: string
    command: string
  }
  /** WSL 发行版演示数据（`session.environment.wsl-distros` 回执形状） */
  wslDistros: string[]
}

/** 设备与配对域种子（票 14） */
export interface PairingDevSeed {
  /** 已配对设备（`session.devices.paired-list` 回执形状） */
  pairedDevices: PairedDeviceInfo[]
  /** 网络信息（`session.network.info` 回执形状） */
  network: LocalNetworkInfo
  /** 当前配对码（`session.pairing.status` 回执形状） */
  pairingCode: PairingCodeInfo | null
  /** QR 连接信息（`session.qr.generate` 回执形状） */
  qr: QrConnectionInfo
  /** 连接历史（`session.devices.history-list` 回执形状） */
  history: ConnectionHistoryEntry[]
  /** 有效期设置（`session.settings.ttl.get` 回执形状） */
  ttls: { pairingCodeTtl: number; qrTokenTtl: number }
}

const devMock: { session: SessionDevSeed; pairing: PairingDevSeed } = {
  session: {
    configs: [
      {
        id: 'mock-config-1',
        name: 'claude-dev',
        environment: 'linux',
        workingDir: '/home/dev/project',
        command: 'claude',
        autoStart: false,
      },
      {
        id: 'mock-config-2',
        name: 'Ubuntu 工具链',
        environment: 'wsl2',
        wslDistro: 'Ubuntu-24.04',
        workingDir: '/mnt/c/Users/binblink/workspace',
        command: 'bash',
        autoStart: true,
      },
    ],
    sessions: [
      {
        id: 'mock-session-1',
        configId: 'mock-config-1',
        name: 'claude-dev',
        status: 'running',
        createdAt: '2026-09-20T02:10:00Z',
        startedAt: '2026-09-20T02:10:05Z',
      },
      {
        id: 'mock-session-2',
        configId: 'mock-config-1',
        name: 'claude-dev(1)',
        status: 'waitingInput',
        createdAt: '2026-09-20T02:20:00Z',
        startedAt: '2026-09-20T02:20:02Z',
      },
      {
        id: 'mock-session-3',
        configId: 'mock-config-2',
        name: 'Ubuntu 工具链',
        status: 'stopped',
        createdAt: '2026-09-20T01:00:00Z',
        startedAt: '2026-09-20T01:00:03Z',
        stoppedAt: '2026-09-20T01:30:00Z',
      },
    ],
    formDefaults: {
      environment: 'linux',
      workingDir: '/home/dev/project',
      command: 'claude',
    },
    wslDistros: ['Ubuntu-24.04', 'Debian'],
  },
  pairing: {
    pairedDevices: [
      {
        id: 'mock-pairing-1',
        deviceName: 'Pixel 9',
        deviceFingerprint: 'fp-pixel-9',
        address: '192.168.1.50:9000',
        pairedAt: '2026-09-18T02:10:00Z',
        lastSeen: '2026-09-20T02:30:00Z',
        connectCount: 7,
      },
      {
        id: 'mock-pairing-2',
        deviceName: 'Reno 12',
        deviceFingerprint: 'fp-reno-12',
        address: '192.168.1.51:9000',
        pairedAt: '2026-09-19T08:00:00Z',
        lastSeen: null,
        connectCount: 1,
      },
    ],
    network: { port: 9000, addresses: ['192.168.1.10', '10.0.0.5'] },
    pairingCode: {
      code: '123456',
      created_at: '2026-09-20T02:30:00Z',
      expires_in: 60,
    },
    qr: {
      host: '192.168.1.10',
      port: 9000,
      token: 'mockqrtoken00000000000000000000',
      remainingSecs: 300,
    },
    history: [
      {
        id: 3,
        deviceId: 'mock-pairing-1',
        authMethod: 'pairing_code',
        result: 'success',
        address: '192.168.1.50:52000',
        connectedAt: '2026-09-20T02:30:00Z',
        disconnectedAt: '2026-09-20T03:05:00Z',
      },
      {
        id: 2,
        deviceId: 'mock-pairing-1',
        authMethod: 'qr',
        result: 'failed',
        address: '192.168.1.50:51000',
        connectedAt: '2026-09-19T10:00:00Z',
        disconnectedAt: null,
      },
      {
        id: 1,
        deviceId: 'mock-pairing-1',
        authMethod: 'biometric',
        result: 'success',
        address: '192.168.1.50:50000',
        connectedAt: '2026-09-18T02:10:00Z',
        disconnectedAt: '2026-09-18T04:00:00Z',
      },
    ],
    ttls: { pairingCodeTtl: 60, qrTokenTtl: 300 },
  },
}

export default devMock
