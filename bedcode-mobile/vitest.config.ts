import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

export default defineConfig({
  plugins: [vue()],
  // 与 vite.config.ts 同源：`__APP_VERSION__` 由构建期注入（真源 tauri.conf.json），
  // 测试侧无注入即 ReferenceError —— 挂载壳屏（设置屏展示版本号）时必须可解析
  define: {
    __APP_VERSION__: JSON.stringify('0.0.0-test'),
  },
  test: {
    environment: 'happy-dom',
    globals: true,
    // 全局装：Tauri core 的 Channel 替身（见 setup.ts 说明）
    setupFiles: ['src/__tests__/setup.ts'],
    // 票 15：终端域测试随源码迁 `wasm-apps/terminal-session/src/terminal/__tests__/`
    // （与宿主 src 同构组织；仍由本配置统一驱动，见 §测试纪律）
    // 票 2026-10-09：宿主页域测试随源码迁 `wasm-apps/terminal-session/src/host/__tests__/`
    include: [
      'src/__tests__/**/*.test.ts',
      'wasm-apps/terminal-session/src/terminal/__tests__/**/*.test.ts',
      'wasm-apps/terminal-session/src/host/__tests__/**/*.test.ts',
      'wasm-apps/terminal-session/src/task/__tests__/**/*.test.ts',
    ],
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