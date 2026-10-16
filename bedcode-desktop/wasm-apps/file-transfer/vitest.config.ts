import { defineConfig } from 'vitest/config'

// file-transfer 前端测试（2026-09-28 起）：独立于宿主 vitest 门禁的最小配置。
// 纯函数测试（taskReason）无需 vue 插件 / happy-dom；跑法：cd wasm-apps/file-transfer
// && pnpm exec vitest run（vitest 经工作区 hoisting 从仓库根解析，见 agent-hub 同款注释）。
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/__tests__/**/*.test.ts'],
  },
})
