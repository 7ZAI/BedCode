/**
 * Ticket 15 — rewrite host i18n keys to plugin-namespace keys, and $t( -> t( in SFC templates.
 * Usage: node migrate-i18n-keys.js <terminalRoot> [--apply]
 */
const fs = require('fs')
const path = require('path')

const ROOT = process.argv[2]
const APPLY = process.argv.includes('--apply')
if (!ROOT) {
  console.error('usage: node migrate-i18n-keys.js <terminalRoot> [--apply]')
  process.exit(1)
}

// ordered: longer / more specific first
const RULES = [
  ["'mobile.terminalHelp.", "'terminalHelp."],
  ['"mobile.terminalHelp.', '"terminalHelp.'],
  ["'mobile.terminal.", "'terminal."],
  ['"mobile.terminal.', '"terminal.'],
  ["'mobile.input.", "'input."],
  ['"mobile.input.', '"input.'],
  ["'mobile.shortcutConfig.", "'shortcutConfig."],
  ['"mobile.shortcutConfig.', '"shortcutConfig.'],
  ["'mobile.shortcutHelp.", "'shortcutHelp."],
  ['"mobile.shortcutHelp.', '"shortcutHelp.'],
  ["'mobile.toolbox.", "'toolbox."],
  ['"mobile.toolbox.', '"toolbox.'],
  ["'mobile.connection.connectFailed'", "'connection.connectFailed'"],
  ["'mobile.session.mockName'", "'session.mockName'"],
  ["'desktop.terminal.title'", "'terminal.titleDesktop'"],
  ["'settings.appearance.followSystem'", "'theme.followSystem'"],
  ["'settings.appearance.darkMode'", "'theme.darkMode'"],
  ["'settings.appearance.lightMode'", "'theme.lightMode'"],
  // template refs: $t('mobile.xxx' -> t('mapped-key'  (mapping applied after $ strip below)
  ["$t('mobile.terminalHelp.", "t('terminalHelp."],
  ['$t("mobile.terminalHelp.', 't("terminalHelp.'],
  ["$t('mobile.terminal.", "t('terminal."],
  ['$t("mobile.terminal.', 't("terminal.'],
  ["$t('mobile.toolbox.", "t('toolbox."],
  ["$t('mobile.shortcutConfig.", "t('shortcutConfig."],
  ["$t('common.button.", "t('common.button."],
]

function walk(dir, out = []) {
  for (const name of fs.readdirSync(dir)) {
    const p = path.join(dir, name)
    const st = fs.statSync(p)
    if (st.isDirectory()) walk(p, out)
    else if (/\.(ts|vue)$/.test(name)) out.push(p)
  }
  return out
}

const files = walk(ROOT)
let touched = 0
for (const file of files) {
  const before = fs.readFileSync(file, 'utf8')
  let src = before
  for (const [from, to] of RULES) src = src.split(from).join(to)
  if (src !== before) {
    touched++
    if (APPLY) fs.writeFileSync(file, src)
    console.log(`${path.relative(ROOT, file)}: changed`)
  }
}
console.log(`\ntotal: ${files.length}, touched: ${touched}, apply=${APPLY}`)
