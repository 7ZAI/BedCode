/**
 * Ticket 15 — rewrite host-absolute imports to plugin-relative paths.
 * Usage: node migrate-imports.js <terminalRoot> [--apply]
 */
const fs = require('fs')
const path = require('path')

const ROOT = process.argv[2]
const APPLY = process.argv.includes('--apply')
if (!ROOT) {
  console.error('usage: node migrate-imports.js <terminalRoot> [--apply]')
  process.exit(1)
}

// [hostPrefix, pluginTarget (relative to terminalRoot)]
const MAP = [
  ['@/utils/frontendLogger', 'host'],
  ['@/utils/terminalMetrics', 'utils/metrics'],
  ['@/utils/terminalDimensions', 'utils/dimensions'],
  ['@/utils/terminalResizeDebouncer', 'utils/resizeDebouncer'],
  ['@/utils/terminalResizePolicy', 'utils/resizePolicy'],
  ['@/utils/terminalScrollback', 'utils/scrollback'],
  ['@/utils/nextPaintFrame', 'utils/nextPaintFrame'],
  ['@/utils/reconnectCountdown', 'utils/reconnectCountdown'],
  ['@/utils/clipboard', 'utils/clipboard'],
  ['@/composables/writeCoalescer', 'utils/writeCoalescer'],
  ['@/config/terminalThemes', 'config/themes'],
  ['@/config/terminalOnboardingSteps', 'config/onboardingSteps'],
  ['@/config/agentPresets', 'config/agentPresets'],
  ['@/stores/terminalBuffer', 'store'],
  ['@/stores/inputAssistant', 'inputAssistant'],
  ['@/composables/terminal/', 'composables/'],
  ['@/composables/useTerminalBuffer', 'composables/useTerminalBuffer'],
  ['@/composables/useTerminalScroll', 'composables/useTerminalScroll'],
  ['@/composables/useTuiCompat', 'composables/useTuiCompat'],
  ['@/composables/useMockTerminal', 'composables/useMockTerminal'],
  ['@/composables/useViewportPanGuard', 'composables/useViewportPanGuard'],
  ['@/composables/useOrientation', 'composables/useOrientation'],
  ['@/composables/model', 'model'],
  ['@/assets/terminal-help', 'assets/terminal-help'],
  ['@/assets/shortcut-help', 'assets/shortcut-help'],
  ['@/styles/terminal.css', 'styles/terminal.css'],
]

// Components handled with a generic rule, except those provided by the host shell (manual).
const COMPONENT_EXCLUDE = new Set(['FileSidebar.vue'])

const SKIP_PREFIXES = [
  '@/composables/useToast',
  '@/composables/useMobileConnection',
  '@/composables/useHttpApi',
  '@/composables/usePresetTasks',
  '@/composables/useTheme',
  '@/composables/useMobileSettings',
  '@/composables/useSwipeTabs',
  '@/composables/useMobileCommands',
  '@/locales',
  '@/stores/settings',
  '@/plugin/',
  '@/components/FileSidebar',
  '@/assets/fonts',
  '@tauri-apps/',
]

function walk(dir, out = []) {
  for (const name of fs.readdirSync(dir)) {
    const p = path.join(dir, name)
    const st = fs.statSync(p)
    if (st.isDirectory()) walk(p, out)
    else if (/\.(ts|vue)$/.test(name) && !/\.d\.ts$/.test(name)) out.push(p)
  }
  return out
}

function toRel(file, target) {
  let rel = path.relative(path.dirname(file), path.join(ROOT, target))
  rel = rel.split(path.sep).join('/')
  if (!rel.startsWith('.')) rel = './' + rel
  return rel
}

function mapSpec(file, spec) {
  // components generic rule
  if (spec.startsWith('@/components/')) {
    const rest = spec.slice('@/components/'.length)
    const seg = rest.split('/')[0]
    if (COMPONENT_EXCLUDE.has(seg)) return null
    return toRel(file, 'components/' + rest)
  }
  for (const [prefix, target] of MAP) {
    if (spec === prefix || spec.startsWith(prefix)) {
      const rest = spec.slice(prefix.length)
      return toRel(file, target + rest)
    }
  }
  if (SKIP_PREFIXES.some((p) => spec.startsWith(p))) return null
  return undefined // unknown
}

const files = walk(ROOT)
const report = []
for (const file of files) {
  let src = fs.readFileSync(file, 'utf8')
  const unresolved = []
  let changed = 0
  src = src.replace(/(from\s+'|import\('|from\s+"|import\(")([^'"]+)('|"|'\)|"\))/g, (m, pre, spec, post) => {
    if (!spec.startsWith('@/') && !spec.startsWith('@tauri-apps/')) return m
    const mapped = mapSpec(file, spec)
    if (mapped === null) {
      if (spec.startsWith('@/')) unresolved.push(spec)
      return m
    }
    if (mapped === undefined) {
      unresolved.push(spec)
      return m
    }
    changed++
    return pre + mapped + post
  })
  // strip .ts suffix in relative specifiers
  src = src.replace(/(from\s+'|import\(')(\.\.?\/[^'"]+)\.ts('|'\))/g, '$1$2$3')
  if (changed || unresolved.length) {
    report.push({ file: path.relative(ROOT, file), changed, unresolved: [...new Set(unresolved)] })
  }
  if (APPLY && changed) fs.writeFileSync(file, src)
}

for (const r of report) {
  console.log(`${r.file}: rewritten=${r.changed}${r.unresolved.length ? ' UNRESOLVED=' + r.unresolved.join(', ') : ''}`)
}
console.log(`\ntotal files: ${files.length}, touched: ${report.length}, apply=${APPLY}`)
