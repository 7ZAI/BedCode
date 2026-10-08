import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    globals: true,
    // 全局装：Tauri core 的 Channel 替身（见 setup.ts 说明）
    setupFiles: ['src/__tests__/setup.ts'],
    // 票 15：终端域测试随源码迁 `wasm-apps/terminal-session/src/terminal/__tests__/`
    // （与宿主 src 同构组织；仍由本配置统一驱动，见 §测试纪律）
    include: ['src/__tests__/**/*.test.ts', 'wasm-apps/terminal-session/src/terminal/__tests__/**/*.test.ts'],
    exclude: ['node_modules', 'dist'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'src/__tests__/']
    }
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
})