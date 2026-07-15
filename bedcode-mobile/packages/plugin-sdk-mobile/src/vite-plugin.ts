/**
 * @bedcode/plugin-sdk-mobile Vite 插件
 */
import type { Plugin, UserConfig } from 'vite'
import MagicString from 'magic-string'

const SHARED_MODULES: Record<string, string> = {
  'vue': 'window.__BEDCODE_SHARED__["vue"]',
  'vue-i18n': 'window.__BEDCODE_SHARED__["vue-i18n"]',
  'pinia': 'window.__BEDCODE_SHARED__["pinia"]',
}

export function bedcodePlugin(): Plugin {
  const externalModules = Object.keys(SHARED_MODULES)

  return {
    name: 'bedcode-shared-modules',
    enforce: 'pre',

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

    renderChunk(code, chunk) {
      let modified = false
      const s = new MagicString(code)

      for (const [modName, globalExpr] of Object.entries(SHARED_MODULES)) {
        const escapedName = modName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

        const defaultRe = new RegExp(
          `import\\s+(\\w+)\\s+from\\s+['"]${escapedName}['"]`, 'gm'
        )
        let match: RegExpExecArray | null
        while ((match = defaultRe.exec(code)) !== null) {
          const varName = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const ${varName} = ${globalExpr}`)
          modified = true
        }

        const namedRe = new RegExp(
          `import\\s*\\{([^}]+)\\}\\s*from\\s+['"]${escapedName}['"]`, 'gm'
        )
        while ((match = namedRe.exec(code)) !== null) {
          const imports = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const { ${imports} } = ${globalExpr}`)
          modified = true
        }

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
