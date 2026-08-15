import type { MessageSchema } from './messages'

/**
 * 中文翻译
 *
 * 独立文件维护，构建期由 Vite 打包内联进 bundle，无运行时文件读取。
 */
const zhCN: MessageSchema = {
  sidebarTitle: 'AI聊天',
}

export default zhCN
