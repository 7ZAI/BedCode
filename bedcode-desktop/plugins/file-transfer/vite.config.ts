import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-desktop/vite'

const pluginId = 'com.bedcode.file-transfer'

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
    // useSettings 的动态 import('@tauri-apps/plugin-dialog') 会触发代码分割
    // 产出额外 chunk，而构建/watch 脚本只同步 index.js 到宿主资源目录，
    // 分包会导致打包后入口 import 缺失文件加载失败——强制内联保持单产物
    rollupOptions: {
      output: { inlineDynamicImports: true },
    },
  },
})
