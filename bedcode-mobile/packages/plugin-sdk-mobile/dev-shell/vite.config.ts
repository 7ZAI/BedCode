/**
 * Dev Shell Vite 配置
 *
 * 环境变量：
 *   BEDCODE_DEV_PLUGINS  逗号分隔的插件说明，每项 `<插件目录>[::<入口文件>]`，
 *                         入口缺省为 `<插件目录>/src/index.ts`（由 bedcode-plugin dev 注入）
 *   BEDCODE_DEV_PORT     端口（缺省 5173，也可用 vite --port 覆盖）
 *
 * 插件前端源码经虚拟模块 virtual:dev-plugins 挂入构建图：
 * - 入口（index.ts）与 plugin.json 均由 vite 解析，插件源码改动走标准 HMR
 * - server.fs.allow 动态放行插件目录（含其 node_modules / SDK file: 软链的真实路径）
 */
import { defineConfig, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import { realpathSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const DEV_SHELL_ROOT = fileURLToPath(new URL('.', import.meta.url))

/** 单个被调试插件 */
export interface DevPluginSpec {
  /** 插件目录（绝对路径） */
  dir: string
  /** 前端入口（绝对路径） */
  entry: string
}

/** 解析 BEDCODE_DEV_PLUGINS 环境变量 */
export function parseDevPlugins(): DevPluginSpec[] {
  const raw = process.env.BEDCODE_DEV_PLUGINS
  if (!raw) return []
  return raw
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => {
      const [dir, entry] = item.split('::')
      const absDir = resolve(dir)
      return {
        dir: absDir,
        entry: entry ? resolve(dir, entry) : resolve(absDir, 'src/index.ts'),
      }
    })
}

/**
 * 虚拟模块：导出被调试插件列表 [{ dir, manifest, entry }]
 *
 * 由 vite 解析 import 语句，插件源码与 plugin.json 进入构建图，天然支持 HMR。
 */
function devPluginsVirtual(plugins: DevPluginSpec[]): Plugin {
  const VIRTUAL_ID = 'virtual:dev-plugins'
  return {
    name: 'bedcode-dev-shell:virtual-plugins',
    resolveId(id) {
      if (id === VIRTUAL_ID) return '\0' + VIRTUAL_ID
    },
    load(id) {
      if (id !== '\0' + VIRTUAL_ID) return
      if (plugins.length === 0) return 'export default []'
      const imports = plugins
        .map((p, i) => {
          const manifestPath = resolve(p.dir, 'plugin.json')
          return (
            `import * as entry${i} from ${JSON.stringify(p.entry)}\n` +
            `import manifest${i} from ${JSON.stringify(manifestPath)}`
          )
        })
        .join('\n')
      const records = plugins
        .map(
          (p, i) =>
            `{ dir: ${JSON.stringify(p.dir)}, manifest: manifest${i}, entry: entry${i} }`,
        )
        .join(',\n  ')
      return `${imports}\n\nexport default [\n  ${records},\n]\n`
    },
  }
}

export default defineConfig(() => {
  const plugins = parseDevPlugins()

  // fs.allow：dev-shell 自身 + 插件目录（file: SDK 依赖为软链，需放行真实路径）
  const allow = new Set<string>([DEV_SHELL_ROOT])
  for (const p of plugins) {
    allow.add(p.dir)
    try {
      allow.add(realpathSync(p.dir))
    } catch {
      // 目录不存在时 vite 会给出更明确的错误，此处仅尽力放行
    }
  }

  return {
    plugins: [vue(), devPluginsVirtual(plugins)],
    resolve: {
      // 强制插件源码与 dev-shell 共用同一份 vue 实例（provide/inject、响应式共享依赖它）
      dedupe: ['vue', 'vue-i18n', 'pinia', 'vue-router'],
    },
    // main.ts 使用 top-level await 串行初始化共享运行时 → 加载插件 → 挂载
    build: {
      target: 'esnext',
    },
    server: {
      port: Number(process.env.BEDCODE_DEV_PORT || 5173),
      fs: { allow: [...allow] },
    },
  }
})
