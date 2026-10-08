/**
 * dev-shell 领域数据：任务队列种子（浏览器 mock 宿主初始队列）
 *
 * 仅 dev-shell 消费（见 PluginDevMock 协议），真实宿主忽略此导出；
 * 由插件入口 index.ts 原样 re-export（dev-shell 从插件入口模块读取 devMock）。
 */
import type { PluginDevMock } from '@binblink/bedcode-plugin-sdk-mobile'

export const devMock: PluginDevMock = {
  queueSeed: [
    {
      id: 'dev-queue-1',
      prompt: '查看当前目录文件列表',
      position: 1,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
    {
      id: 'dev-queue-2',
      prompt: '输出系统信息',
      position: 2,
      status: 'pending',
      created_at: new Date().toISOString(),
    },
  ],
}
