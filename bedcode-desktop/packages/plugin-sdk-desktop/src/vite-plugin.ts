/**
 * @binblink/plugin-sdk-desktop Vite 插件
 *
 * 处理插件构建时的共享模块外部化：
 * 1. 将 vue/vue-i18n/pinia 标记为 rollup external
 * 2. 构建后将 import 语句替换为 window.__BEDCODE_SHARED__ 读取
 */
import type { Plugin, UserConfig } from 'vite'
import MagicString from 'magic-string'

/** 共享模块映射：模块名 → 全局变量访问表达式 */
const SHARED_MODULES: Record<string, string> = {
  'vue': 'window.__BEDCODE_SHARED__["vue"]',
  'vue-i18n': 'window.__BEDCODE_SHARED__["vue-i18n"]',
  'pinia': 'window.__BEDCODE_SHARED__["pinia"]',
}

/**
 * BedCode 插件构建 Vite 插件
 *
 * 在插件 vite.config.ts 中使用：
 * ```ts
 * import { bedcodePlugin } from '@binblink/plugin-sdk-desktop/vite'
 * export default defineConfig({
 *   plugins: [vue(), bedcodePlugin()],
 *   build: { ... }
 * })
 * ```
 */
export function bedcodePlugin(): Plugin {
  const externalModules = Object.keys(SHARED_MODULES)

  return {
    name: 'bedcode-shared-modules',
    enforce: 'pre',

    // 注入 rollup external 配置
    config(config: UserConfig) {
      const existingExternal = config.build?.rollupOptions?.external
      const externalArray = Array.isArray(existingExternal)
        ? existingExternal
        : typeof existingExternal === 'string'
          ? [existingExternal]
          : []

      return {
        build: {
          rollupOptions: {
            external: [...externalArray, ...externalModules],
          },
        },
      }
    },

    // 构建后替换 import 为全局变量读取
    renderChunk(code, chunk) {
      let modified = false
      const s = new MagicString(code)

      for (const [modName, globalExpr] of Object.entries(SHARED_MODULES)) {
        const escapedName = modName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

        // import X from 'vue' → const X = window.__BEDCODE_SHARED__["vue"]
        const defaultRe = new RegExp(
          `import\\s+(\\w+)\\s+from\\s+['"]${escapedName}['"]`, 'gm'
        )
        let match: RegExpExecArray | null
        while ((match = defaultRe.exec(code)) !== null) {
          const varName = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const ${varName} = ${globalExpr}`)
          modified = true
        }

        // import { X, Y } from 'vue' → const { X, Y } = window.__BEDCODE_SHARED__["vue"]
        const namedRe = new RegExp(
          `import\\s*\\{([^}]+)\\}\\s*from\\s+['"]${escapedName}['"]`, 'gm'
        )
        while ((match = namedRe.exec(code)) !== null) {
          const imports = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const { ${imports} } = ${globalExpr}`)
          modified = true
        }

        // import * as X from 'vue' → const X = window.__BEDCODE_SHARED__["vue"]
        const namespaceRe = new RegExp(
          `import\\s+\\*\\s+as\\s+(\\w+)\\s+from\\s+['"]${escapedName}['"]`, 'gm'
        )
        while ((match = namespaceRe.exec(code)) !== null) {
          const varName = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const ${varName} = ${globalExpr}`)
          modified = true
        }
      }

      if (!modified) return null

      return {
        code: s.toString(),
        map: s.generateMap({ hires: true }),
      }
    },
  }
}
