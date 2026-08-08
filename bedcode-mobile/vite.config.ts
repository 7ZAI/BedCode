import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { readFileSync } from 'fs'

const host = process.env.TAURI_DEV_HOST

// 从 tauri.conf.json 读取应用版本（作为版本号的唯一来源）
const tauriConf = JSON.parse(readFileSync(resolve(__dirname, 'src-tauri/tauri.conf.json'), 'utf-8'))
const appVersion = tauriConf.version || '0.0.0'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    host: host ? '0.0.0.0' : '0.0.0.0',
    port: 1423,
    strictPort: true,
    watch: {
      // 排除巨型构建目录，避免 chokidar 扫描/监听数万文件霸占事件循环导致请求挂起
      ignored: [
        '**/src-tauri/target/**',
        '**/src-tauri/gen/**',
        '**/rust/target/**',
        '**/dist/**',
        '**/node_modules/**',
        '**/.git/**',
      ],
    },
    hmr: host
      ? {
          protocol: 'ws',
          host: host,
          port: 1424,
        }
      : undefined,
    fs: {
      allow: [
        resolve(__dirname, 'src'),
        // 插件 SDK 源码（@bedcode/plugin-sdk-mobile 经 file: symlink 解析为真实路径，
        // 共享 UI 组件如 Select.vue 在 packages/ 下，dev server 需显式放行）
        resolve(__dirname, 'packages'),
        resolve(__dirname, 'index.html'),
        resolve(__dirname, 'public'),
        resolve(__dirname, 'node_modules'),
        resolve(__dirname, 'package.json'),
        resolve(__dirname, 'vite.config.ts'),
        resolve(__dirname, 'tailwind.config.js'),
        resolve(__dirname, 'postcss.config.js'),
        resolve(__dirname, 'tsconfig.json'),
      ],
    },
  },
  optimizeDeps: {
    entries: ['./index.html'],
    include: [
      'vue',
      'vue-router',
      'pinia',
      '@tauri-apps/api/core',
      '@tauri-apps/api/event',
      '@tauri-apps/api/window',
      '@tauri-apps/plugin-dialog',
      '@tauri-apps/plugin-os',
      '@tauri-apps/plugin-notification',
      '@skipperndt/plugin-machine-uid',
      'ansi_up',
      'html5-qrcode',
      'uuid',
    ],
    holdUntilCrawlEnd: false,
  },
  build: {
    rollupOptions: {
      output: {
        chunkFileNames: (chunkInfo) => {
          // 插件 chunk 输出到 plugins/ 目录
          if (chunkInfo.name?.startsWith('plugins/')) {
            return `${chunkInfo.name}.js`
          }
          return 'assets/[name]-[hash].js'
        },
      },
    },
  },
  clearScreen: false,
})
