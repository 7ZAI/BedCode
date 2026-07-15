import { defineConfig } from 'vite'
import { resolve } from 'path'
import { bedcodePlugin } from '@bedcode/plugin-sdk-desktop/vite'

export default defineConfig({
  plugins: [bedcodePlugin()],
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
  },
})
