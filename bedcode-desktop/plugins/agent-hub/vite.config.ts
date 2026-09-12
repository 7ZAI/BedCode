import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-desktop/vite'

const pluginId = 'com.bedcode.agent-hub'

export default defineConfig({
  plugins: [vue(), bedcodePlugin()],
  define: {
    'process.env.NODE_ENV': JSON.stringify('production'),
  },
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: resolve(__dirname, 'dist'),
    emptyOutDir: true,
    minify: 'terser',
    // 插件产物契约 = 单文件 index.js（manifest.main，宿主按目录整包分发）：
    // 强制内联避免动态 import 分包导致入口缺失（同 file-transfer）
    rollupOptions: {
      output: { inlineDynamicImports: true },
    },
  },
})
