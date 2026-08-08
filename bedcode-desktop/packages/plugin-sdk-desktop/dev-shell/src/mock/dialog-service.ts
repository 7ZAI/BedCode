/**
 * 最小对话框服务（桌面端 dev-shell）
 *
 * 桌面端插件 context 无 dialogs API，仅 fileService 的 pick 系列使用；
 * 提供 prompt 队列由 PromptHost.vue 渲染（与移动端 dialog-service 同构，只保留 prompt）。
 */
import { ref } from 'vue'

export interface PromptItem {
  id: number
  title: string
  message?: string
  placeholder?: string
  value?: string
  resolve: (value: string | null) => void
}

const queue = ref<PromptItem[]>([])
let nextId = 0

function showPrompt(options: {
  title: string
  message?: string
  inputPlaceholder?: string
  inputValue?: string
}): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    const id = ++nextId
    queue.value.push({
      id,
      title: options.title,
      message: options.message,
      placeholder: options.inputPlaceholder,
      value: options.inputValue,
      resolve,
    })
  })
}

/** 解析队列顶部条目 */
export function resolveTop(value: string | null): void {
  const item = queue.value.shift()
  if (item) item.resolve(value)
}

export const dialogService = { queue, showPrompt }
