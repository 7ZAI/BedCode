/**
 * vitest 全局测试装（setupFiles）
 *
 * 为什么需要：`@tauri-apps/api/core` 的 `Channel` 在构造时会调用
 * `window.__TAURI_INTERNALS__.transformCallback`（仅真实 WebView 注入），happy-dom
 * 下为 undefined —— 终端缓冲 store 的 `markPageEntered` 一创建段2 推送通道就抛
 * `Cannot read properties of undefined (reading 'transformCallback')`。
 *
 * 统一替身只保留 `onmessage` 槽：测试可直接向经由 mocked `terminalPageSubscribe`
 * 传出的通道投喂 TB v3 二进制帧，驱动与真机同一条解析路径。IPC 命令面不在此伪造
 * （各测试按需 mock `@/composables/useMobileCommands`）。
 */
import { vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => {}),
  Channel: class ChannelMock<T> {
    onmessage: ((message: T) => void) | null = null
  },
}))
