/**
 * Ticket 15 — copy terminal-domain tests from host src/__tests__ into the plugin.
 * Rewrites @/ specifiers to plugin-relative paths.
 * Usage: node migrate-tests.js [--apply]
 */
const fs = require('fs')
const path = require('path')

const ROOT = '/home/binblink/project/tauriProject/BedCode/bedcode-mobile'
const DEST = path.join(ROOT, 'plugins/terminal-session/src/terminal/__tests__')
const APPLY = process.argv.includes('--apply')

// [host source rel to src/__tests__, dest rel to __tests__]
const FILES = [
  ['composables/useTerminalBuffer.test.ts', 'useTerminalBuffer.test.ts'],
  ['composables/useTerminalScroll.test.ts', 'useTerminalScroll.test.ts'],
  ['composables/useTuiCompat.test.ts', 'useTuiCompat.test.ts'],
  ['composables/useViewportPanGuard.test.ts', 'useViewportPanGuard.test.ts'],
  ['composables/writeCoalescer.test.ts', 'writeCoalescer.test.ts'],
  ['composables/terminal/useTerminalKeyboardAvoidance.test.ts', 'useTerminalKeyboardAvoidance.test.ts'],
  ['composables/terminal/useTerminalSubscription.test.ts', 'useTerminalSubscription.test.ts'],
  ['stores/terminalBuffer.test.ts', 'store.test.ts'],
  ['integration/terminal-flow.test.ts', 'terminalFlow.test.ts'],
  ['config/agentPresets.test.ts', 'agentPresets.test.ts'],
  ['config/terminalOnboardingSteps.test.ts', 'onboardingSteps.test.ts'],
  ['config/terminalThemes.test.ts', 'themes.test.ts'],
  ['utils/terminalDimensions.test.ts', 'dimensions.test.ts'],
  ['utils/terminalMetrics.test.ts', 'metrics.test.ts'],
  ['utils/terminalResizeDebouncer.test.ts', 'resizeDebouncer.test.ts'],
  ['utils/terminalResizePolicy.test.ts', 'resizePolicy.test.ts'],
  ['utils/reconnectCountdown.test.ts', 'reconnectCountdown.test.ts'],
  ['components/terminalInputBarBlur.test.ts', 'terminalInputBarBlur.test.ts'],
  ['components/terminalInputBarHeight.test.ts', 'terminalInputBarHeight.test.ts'],
  ['integration/fixtures/terminalInputBarBlurHost.vue', 'fixtures/terminalInputBarBlurHost.vue'],
  ['composables/terminal/fixtures/useTerminalSubscriptionHost.vue', 'fixtures/useTerminalSubscriptionHost.vue'],
]

const MAP = [
  ['@/composables/terminal/', '../composables/'],
  ['@/composables/useTerminalBuffer', '../composables/useTerminalBuffer'],
  ['@/composables/useTerminalScroll', '../composables/useTerminalScroll'],
  ['@/composables/useTuiCompat', '../composables/useTuiCompat'],
  ['@/composables/useViewportPanGuard', '../composables/useViewportPanGuard'],
  ['@/composables/useMockTerminal', '../composables/useMockTerminal'],
  ['@/composables/writeCoalescer', '../utils/writeCoalescer'],
  ['@/stores/terminalBuffer', '../store'],
  ['@/stores/inputAssistant', '../inputAssistant'],
  ['@/utils/terminalMetrics', '../utils/metrics'],
  ['@/utils/terminalDimensions', '../utils/dimensions'],
  ['@/utils/terminalResizeDebouncer', '../utils/resizeDebouncer'],
  ['@/utils/terminalResizePolicy', '../utils/resizePolicy'],
  ['@/utils/reconnectCountdown', '../utils/reconnectCountdown'],
  ['@/utils/clipboard', '../utils/clipboard'],
  ['@/utils/frontendLogger', '../host'],
  ['@/config/terminalThemes', '../config/themes'],
  ['@/config/terminalOnboardingSteps', '../config/onboardingSteps'],
  ['@/config/agentPresets', '../config/agentPresets'],
  ['@/components/TerminalInputBar.vue', '../components/TerminalInputBar.vue'],
  ['@/components/TerminalHeader.vue', '../components/TerminalHeader.vue'],
  ['@/components/ConfirmDialog.vue', '../components/ConfirmDialog.vue'],
  ['@/components/TaskPickerModal.vue', '../components/TaskPickerModal.vue'],
  ['@/composables/model', '../model'],
]

const unresolved = []
for (const [srcRel, destRel] of FILES) {
  const srcPath = path.join(ROOT, 'src/__tests__', srcRel)
  if (!fs.existsSync(srcPath)) {
    console.log(`MISSING SOURCE: ${srcRel}`)
    continue
  }
  let src = fs.readFileSync(srcPath, 'utf8')
  src = src.replace(/\r\n/g, '\n')
  for (const [from, to] of MAP) src = src.split(from).join(to)
  for (const m of src.matchAll(/['"]@\/([^'"]+)['"]/g)) unresolved.push(`${destRel}: @/${m[1]}`)
  // relative imports between tests (e.g. fixtures) stay valid only if layout matches; report them
  if (APPLY) {
    const out = path.join(DEST, destRel)
    fs.mkdirSync(path.dirname(out), { recursive: true })
    fs.writeFileSync(out, src)
  }
  console.log(`${APPLY ? 'copied' : 'dry'} ${srcRel} -> ${destRel}`)
}

if (unresolved.length) {
  console.log('\nUNRESOLVED @/ refs:')
  for (const u of unresolved) console.log('  ' + u)
} else {
  console.log('\nno unresolved @/ refs')
}
