#!/usr/bin/env node
// 把 src 内联的 `#[cfg(test)] mod tests { ... }` 按「功能分区」拆成多个 .rs 文件。
//
// 两种落点（对应 AGENTS.md「单元测试纪律」与仓库既有惯例）：
//   --target private  →  src/<mod>/tests/<name>.rs
//                        源文件尾部留 `#[cfg(test)] mod tests { use super::*; mod <name>; ... }`，
//                        子模块路径由 rustc 自动解析到 `<mod>/tests/<name>.rs`（内联模块的
//                        模块目录 = 文件名同名目录），私有项可见性与内联形态完全等价。
//   --target public   →  <crate>/tests/<name>.rs（独立集成测试 crate，只准碰 pub API）
//
// 分区依据：顶层 `// ==================== 标题 ====================` 分隔注释；
// 没有分隔注释时按「每 N 个 #[test] 一组」兜底切分，避免产出单个巨型文件。
//
// 用法：
//   node scripts/split-rust-tests.mjs --file <path.rs> --target private
//   node scripts/split-rust-tests.mjs --file <path.rs> --target public --lib <crate_name>
//   node ... --dry            # 只打印计划，不落盘
//   node ... --mod <name>     # 指定测试模块名（默认 tests）

import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs'
import { dirname, join, basename, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { findTestBlocks } from './audit-rust-tests.mjs'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')

// ==================== 参数 ====================

const argv = process.argv.slice(2)
const getArg = (k, d = null) => {
  const i = argv.indexOf(`--${k}`)
  return i >= 0 && argv[i + 1] ? argv[i + 1] : d
}
const hasFlag = (k) => argv.includes(`--${k}`)

const fileArg = getArg('file')
if (!fileArg) {
  console.error('缺少 --file')
  process.exit(2)
}
const target = getArg('target', 'private')
const modName = getArg('mod', 'tests')
const libName = getArg('lib')
const dry = hasFlag('dry')
const perGroup = Number(getArg('per-group', 6))

const FILE = join(ROOT, fileArg)
if (!existsSync(FILE)) {
  console.error(`文件不存在：${fileArg}`)
  process.exit(2)
}

// ==================== 顶层切块 ====================

const SECTION_RE = /^\s*\/\/ ={4,}\s*(.+?)\s*={4,}\s*$/
const TEST_ATTR_RE = /^\s*#\[(?:tokio::)?test(?:\s*\([^)]*\))?\]\s*$/
const ITEM_RE = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:fn|struct|enum|trait|type|static|union|impl|macro_rules)\s+(\w+)/
const USE_RE = /^\s*(?:pub(?:\([^)]*\))?\s+)?use\s/

/** `([{` 相对 `)]}` 的净深度（use 语句终止判定用） */
function balDelta(s) {
  const open = (s.match(/[[({]/g) || []).length
  const close = (s.match(/[\])}]/g) || []).length
  return open - close
}

/**
 * 把模块体切成顶层 chunk：{ kind, name, lines, section, attrs }
 *
 * `struct` 行（注释/字符串已清空）用于算括号与判类型，`raw` 行用于输出——
 * 两者必须分开：用原文算括号会被字符串/注释里的 `{` 撑破配平。
 */
function topLevelChunks(struct, raw) {
  const chunks = []
  let cur = null
  let depth = 0
  let pending = []
  let i = 0
  const flush = () => {
    if (cur) chunks.push(cur)
    cur = null
  }
  const start = (name, kind) => {
    cur = { name, kind, lines: [...pending], depth: 0, sawOpen: false, started: false }
    pending = []
  }
  /** chunk 何时结束：见到配平的 {}，或（无花括号的声明）首行就以 ; 收尾。
   *  `;` 只在首行生效：多行签名（`fn f(\n  a: T,\n) -> R {`）中途不能被判收尾。*/
  const maybeFlush = (rawLine) => {
    if (cur.sawOpen && depth === 0) { flush(); return }
    if (!cur.sawOpen && !cur.started && depth === 0 && rawLine.trimEnd().endsWith(';')) flush()
  }
  while (i < struct.length) {
    const s = struct[i]
    const delta = (s.match(/\{/g) || []).length - (s.match(/\}/g) || []).length
    if (!cur) {
      const trimmed = s.trim()
      // 空行/注释/属性都不自成块：它们是下一个 item 的前缀，否则会把 `#[test]`
      // 挂到空行块上，isTestChunk 就再也认不出用例了。
      // 注释判定必须看原文：stripped 行里注释已被抹成空格，会被下面的空行分支吃掉。
      if (raw[i].trim().startsWith('//')) {
        const sec = SECTION_RE.exec(raw[i])
        pending.push({ section: sec ? sec[1] : null, raw: raw[i] })
        i += 1
        continue
      }
      if (trimmed === '') { i += 1; continue }
      if (/^\s*#\[/.test(s)) { pending.push({ section: null, raw: raw[i] }); i += 1; continue }
      const u = USE_RE.test(s)
      const im = ITEM_RE.exec(s)
      if (u) {
        // 单行 use（以 ; 收尾）就是一条；只有真正未收尾的才往下续行。
        // 终止条件用「已配平且本行以 ; 结尾」：只看 ; 会被空行/被抹掉的注释行带跑。
        start(null, 'use')
        cur.lines.push({ section: null, raw: raw[i] })
        let j = i + 1
        if (!s.trimEnd().endsWith(';')) {
          let bal = balDelta(s)
          while (j < struct.length && (bal !== 0 || !struct[j].trimEnd().endsWith(';'))) {
            cur.lines.push({ section: null, raw: raw[j] })
            bal += balDelta(struct[j])
            j += 1
          }
          if (j < struct.length) cur.lines.push({ section: null, raw: raw[j] })
          i = j + 1
        } else {
          i = j // 单行：下一行照常处理（跳过一行的写法会吞掉后续 use）
        }
        cur.depth = 0
        depth = 0
        flush()
        continue
      }
      if (im) start(im[1], 'item')
      else if (/^\s*(?:pub\s+)?mod\s+\w+\s*[;{]/.test(s)) start(null, 'mod')
      else start(null, 'other')
      cur.lines.push({ section: null, raw: raw[i] })
      cur.started = true
      cur.depth = delta
      depth += delta
      if (delta > 0) cur.sawOpen = true
      maybeFlush(raw[i])
      i += 1
      continue
    }
    cur.lines.push({ section: null, raw: raw[i] })
    cur.started = true
    cur.depth += delta
    depth += delta
    if (delta > 0) cur.sawOpen = true
    maybeFlush(raw[i])
    i += 1
  }
  flush()
  return chunks
}

/** 顶层块是不是 #[test] / #[tokio::test] 用例 */
function isTestChunk(c) {
  return c.kind === 'item' && c.lines.some((l) => TEST_ATTR_RE.test(l.raw))
}

// ==================== 分组 ====================

const src = readFileSync(FILE, 'utf8')
const { blocks } = findTestBlocks(src)
const block = blocks.find((b) => b.name === modName)
if (!block) {
  console.error(`未找到 #[cfg(test)] mod ${modName}（可选：${blocks.map((b) => b.name).join(', ')}）`)
  process.exit(2)
}
if (block.kind !== 'inline') {
  console.error('该测试模块已是 #[path] 挂载形态，先手工归并再拆')
  process.exit(2)
}

// 结构行与原文行必须逐行对应（stripNonCode 保留换行就是为此）；不对齐直接停，
// 否则 chunk 内容会张冠李戴，表现为莫名其妙的括号不配平。
if ((block.rawBody ?? block.body).length !== block.body.length) {
  console.error(`结构行与原文行数不一致（${block.body.length} vs ${(block.rawBody ?? block.body).length}），已中止`)
  process.exit(3)
}

const chunks = topLevelChunks(block.body, block.rawBody ?? block.body)
if (hasFlag('debug')) {
  const byKind = {}
  for (const c of chunks) {
    const k = `${c.kind}${isTestChunk(c) ? '+test' : ''}`
    byKind[k] = byKind[k] ?? { n: 0, lines: 0, sample: null }
    byKind[k].n += 1
    byKind[k].lines += c.lines.length
    if (!byKind[k].sample) byKind[k].sample = c.lines[0]?.raw.trim().slice(0, 60)
  }
  console.log('\n-- chunk 分布 --')
  for (const [k, v] of Object.entries(byKind)) {
    console.log(`${k.padEnd(14)} ${String(v.n).padStart(4)} 块 ${String(v.lines).padStart(5)} 行   例：${v.sample}`)
  }
  console.log(`总 chunk ${chunks.length}，用例 ${chunks.filter(isTestChunk).length}\n`)
}

// 维护「当前分区」游标：分隔注释出现在某个 chunk 之前 → 该 chunk 起新分区
const groups = []
let current = null
for (const c of chunks) {
  const inlineSection = c.lines.find((l) => l.section)?.section
  if (inlineSection && current) groups.push(current)
  if (inlineSection) current = { section: inlineSection, tests: [], helpers: [], uses: [] }
  if (!current) current = { section: 'general', tests: [], helpers: [], uses: [] }
  if (c.kind === 'use') current.uses.push(c)
  else if (isTestChunk(c)) current.tests.push(c)
  // 其余（辅助 fn / const / struct / 无法归类的杂项）一律进 helpers——不得丢行
  else current.helpers.push(c)
}
if (current) groups.push(current)

// 去掉只有辅助代码、没有用例的分组（uses / lead 回收到首个真分组）
// 前导的「零用例分组」（imports + 共享脚手架）并入首个真分组，不能丢
const leading = { uses: [], helpers: [] }
const real = []
for (const g of groups) {
  if (g.tests.length === 0) {
    if (real.length) real[real.length - 1].helpers.push(...g.helpers)
    else { leading.uses.push(...g.uses); leading.helpers.push(...g.helpers) }
    continue
  }
  if (!real.length) real.push({ ...g, uses: [...leading.uses, ...g.uses], helpers: [...leading.helpers, ...g.helpers] })
  else real.push(g)
}
// 用例组过大 → 按数量再切
const finalGroups = []
for (const g of real) {
  if (g.tests.length <= perGroup) { finalGroups.push(g); continue }
  for (let i = 0; i < g.tests.length; i += perGroup) {
    finalGroups.push({ section: g.section, tests: g.tests.slice(i, i + perGroup), helpers: [], uses: [] })
  }
  finalGroups[finalGroups.length - 1].helpers = g.helpers
}
for (const g of finalGroups) g.section = g.section || 'general'

// ==================== 命名 ====================

function slug(s) {
  const ascii = s
    .replace(/[^A-Za-z0-9_]+/g, ' ')
    .replace(/\s+/g, '_')
    .replace(/^_+|_+$/g, '')
    .toLowerCase()
  return ascii
}
function uniqName(base, used) {
  let n = base
  let i = 2
  while (used.has(n)) n = `${base}_${i++}`
  used.add(n)
  return n
}

const renames = new Map()
for (let i = 0; i < argv.length; i += 1) {
  if (argv[i] === '--rename' && argv[i + 1]) {
    const [from, to] = argv[i + 1].split('=')
    if (from && to) renames.set(from.trim(), to.trim())
  }
}

// 目录模块的入口是 `<dir>/mod.rs`：basename 会得到 'mod'，必须换成目录名，
// 否则产物会落进 `<dir>/mod/tests/`（rs 找不到文件，报 E0583）。
const fileBase = basename(FILE, '.rs')
const modStem = fileBase === 'mod' ? basename(dirname(FILE)) : fileBase

/** 组内首个用例的函数名（去掉 test 后缀）——CJK 标题/纯数字标题取名时的依据 */
function firstTestName(g) {
  const first = g.tests[0]
  if (!first) return ''
  const m = first.lines.map((l) => l.raw).join('\n').match(/\bfn\s+(\w+)\s*\(/)
  return m ? m[1] : ''
}

const used = new Set()
/** 模块名过长会让路径难读；按词边界截到 28 字符 */
function capName(s) {
  if (s.length <= 28) return s
  const cut = s.slice(0, 28)
  const at = cut.lastIndexOf('_')
  return (at > 12 ? cut.slice(0, at) : cut).replace(/_+$/, '')
}
for (const [i, g] of finalGroups.entries()) {
  const renamed = renames.get(g.section)
  // 优先级：--rename > 标题 slug > 首个用例名 > group_N
  let short = renamed ?? slug(g.section).replace(new RegExp(`^${slug(modStem)}_`), '')
  // 分隔标题常以票号开头（“票 02：…”→ slug 只剩 “02”）；首个分区没有标题（general）
  // ——两种碎片名都回退到用例名
  if (!renamed && (!short || /^\d+$/.test(short) || short === 'general')) {
    const fn = firstTestName(g).replace(/_(test|it)$/, '')
    short = slug(fn) || short
  }
  g.name = uniqName(capName(slug(short)) || `group_${i + 1}`, used)
}
// 公共目标平铺在 <crate>/tests/，不同源模块会撞名——统一加模块前缀
if (target === 'public' && getArg('no-prefix') !== '1') {
  const seen = new Set()
  for (const g of finalGroups) {
    const base = `${slug(modStem)}_${g.name}`
    g.name = uniqName(base.replace(/_\d+$/, (m) => m), seen)
  }
}

// ==================== 渲染 ====================

const bodyLines = (c) => c.lines.map((l) => l.raw)

// 模块路径：src/a/b/c.rs → a::b::c（lib.rs / main.rs → 根）
function crateModulePath(fileAbs) {
  const parts = relative(ROOT, fileAbs).split('/')
  const crateIdx = parts.findIndex((p) => p === 'src')
  const segs = parts.slice(crateIdx + 1, -1)
  // 目录模块：src/policy/mod.rs → policy（去掉结尾的 mod 段）
  if (segs.length && segs[segs.length - 1] === 'mod') segs.pop()
  if (segs.length && (segs[segs.length - 1] === 'lib' || segs[segs.length - 1] === 'main')) segs.pop()
  return segs.length ? `crate::${segs.join('::')}::${modStem}` : `crate::${modStem}`
}

const commonUseChunks = finalGroups.flatMap((g) => g.uses)
const commonUses = [...new Set(commonUseChunks.map((c) => bodyLines(c).join('\n')))]
const allHelpers = finalGroups.flatMap((g) => g.helpers)
const helperNames = allHelpers.map((c) => c.name).filter(Boolean)

const usePath = getArg('use-path') ?? (libName ? `${libName}::${crateModulePath(FILE).replace('crate::', '')}` : null)

/** 脚手架单独成文件（共享项多时避免 entry 变成第二个巨型块） */
const scaffoldName = 'scaffold'
const useScaffoldFile = target === 'private' && allHelpers.length >= Number(getArg('scaffold-threshold', '8'))

/** 模块体内缩进（`mod tests {` 里一律 4 空格）→ 落盘文件顶格 */
function dedent(lines) {
  const body = lines.filter((l) => l.trim() !== '')
  if (!body.length) return lines
  const min = Math.min(...body.map((l) => (/^ */.exec(l)[0] || '').length))
  return lines.map((l) => (l.trim() === '' ? '' : l.slice(min)))
}

/** 去掉块首尾残留的分隔注释（分组已由文件名承载） */
function stripSectionMarkers(lines) {
  let a = 0
  let b = lines.length - 1
  while (a <= b && (lines[a].trim() === '' || SECTION_RE.test(lines[a]))) a += 1
  while (b >= a && (lines[b].trim() === '' || SECTION_RE.test(lines[b]))) b -= 1
  return lines.slice(a, b + 1)
}

/** 只保留本文件真正用到的导入（按标识符出现与否过滤），避免拆分后一串 unused_imports */
function filterUses(uses, bodyText) {
  const out = []
  for (const u of uses) {
    const srcText = typeof u === 'string' ? u : bodyLines(u).join('\n')
    const text = srcText.split('\n').map((l) => l.trim()).join(' ')
    const m = /^use\s+(.+?);$/.exec(text)
    if (!m) continue
    const target = m[1]
    if (/\*\s*$/.test(target) || !target.includes('{')) {
      // 简单 use / glob：末段标识符出现就保留
      const last = target.replace(/::\*$/, '').split('::').pop()
      if (/\*/.test(target) || new RegExp(`\\b${last}\\b`).test(bodyText)) out.push(srcText)
      continue
    }
    const braceStart = target.indexOf('{')
    // base 自带结尾的 `::`（如 `a::b::{X}` → `a::b::`），不要再剥
    const base = target.slice(0, braceStart)
    const inner = target.slice(braceStart + 1, target.lastIndexOf('}'))
    const kept = inner
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
      .filter((name) => {
        const ident = name.split(' as ').pop().trim()
        return new RegExp(`\\b${ident}\\b`).test(bodyText)
      })
    if (kept.length) out.push(`use ${base}{${kept.join(', ')}};`)
  }
  return out
}

function renderFile(g, { scaffold = false } = {}) {
  const bodyText = [...g.tests.flatMap(bodyLines), ...g.helpers.flatMap(bodyLines)].join('\n')
  const out = []
  out.push(`//! ${g.section} — ${target === 'public' ? '公共 API 集成测试' : 'crate 内单元测试'}（自 ${relative(ROOT, FILE)} 迁出）`)
  out.push('')
  if (target === 'public') {
    out.push(`use ${usePath}::*;`)
  } else {
    out.push('use super::*;')
  }
  if (scaffold) out.push(`use super::${scaffoldName}::*;`)
  out.push('')
  // 非 glob 的 import：entry 的私有 import 不会 glob 传递给子模块，每个文件自带一份
  const ownUses = filterUses(
    g.uses.length ? g.uses : commonUseChunks,
    bodyText,
  ).filter((u) => !/use\s+super\s*::\s*\*\s*;/.test(u))
  for (const u of ownUses) out.push(u.split('\n').map((l) => (l.trim() ? l.trim() : l)).join('\n'))
  if (ownUses.length) out.push('')
  const body = dedent(stripSectionMarkers([...g.tests.flatMap(bodyLines)]))
  for (const l of body) out.push(l)
  return out.join('\n').replace(/\n{3,}/g, '\n\n').trimEnd() + '\n'
}

/** 共享脚手架文件：对外 `pub(super)`，用例经 `use super::scaffold::*` 拿 */
/** inherent impl（非 `impl Trait for`）里的方法也要 pub(super)：结构体在脚手架模块，
 *  调用方是兄弟模块。trait 实现不允许写可见性，必须排除。 */
function widenInherentImpls(text) {
  const lines = text.split('\n')
  let implDepth = null
  for (let i = 0; i < lines.length; i += 1) {
    const delta = (lines[i].match(/\{/g) || []).length - (lines[i].match(/\}/g) || []).length
    if (implDepth === null) {
      if (/^impl\b/.test(lines[i]) && lines[i].includes('{') && !/\bfor\b/.test(lines[i])) {
        implDepth = 0
      }
      continue
    }
    if (implDepth === 0) {
      const m = /^(\s+)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+/.exec(lines[i])
      if (m) {
        lines[i] = lines[i].replace(/^(\s+)(?:pub(?:\([^)]*\))?\s+)?/, (_s, sp) => `${sp}pub(super) `)
      }
    }
    implDepth += delta
    if (implDepth < 0) implDepth = null
  }
  return lines.join('\n')
}

/** 顶层具名 struct 的字段也要 pub(super)（调用方在兄弟模块里构造该结构体） */
function widenTopLevelStructFields(text) {
  const lines = text.split('\n')
  let depth = null
  for (let i = 0; i < lines.length; i += 1) {
    const delta = (lines[i].match(/\{/g) || []).length - (lines[i].match(/\}/g) || []).length
    if (depth === null) {
      // 元组结构体（`struct X(T);`）字段天然是 pub，跳过；只处理具名体
      if (/^(?:pub(?:\([^)]*\))?\s+)?struct\s+\w+\s*\{\s*$/.test(lines[i])) depth = 0
      continue
    }
    if (depth === 0 && /^\s+(?!pub\b|\/\/|#\[)/.test(lines[i]) && /^(\s+)([A-Za-z_]\w*)\s*:/.test(lines[i])) {
      lines[i] = lines[i].replace(/^(\s+)/, '$1pub(super) ')
    }
    depth += delta
    if (depth < 0) depth = null
  }
  return lines.join('\n')
}

function renderScaffold() {
  const helpersText = allHelpers.flatMap(bodyLines).join('\n')
  const out = [
    `//! ${relative(ROOT, FILE)} 的跨分组测试脚手架（用例文件经 \`use super::scaffold::*\` 引用）`,
    '',
    'use super::*;',
    '',
  ]
  const uses = filterUses(commonUseChunks, helpersText).filter((u) => !/use\s+super\s*::\s*\*\s*;/.test(u))
  for (const u of uses) out.push(u.split('\n').map((l) => (l.trim() ? l.trim() : l)).join('\n'))
  if (uses.length) out.push('')
  for (const c of allHelpers) {
    for (const l of dedent(bodyLines(c))) out.push(l)
    out.push('')
  }
  let text = out.join('\n').replace(/\n{3,}/g, '\n\n').trimEnd() + '\n'
  if (target === 'private') {
    // 只改**顶格**（顶��� item）：impl 块内的方法（含 trait 实现）不允许写可见性，
    // 子模块经 `use super::scaffold::*` 取顶层项即可。
    const stripPub = (s) => s.replace(/^(pub(?:\([^)]*\))?\s+)?/, '')
    text = text
      .replace(/^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+/gm, (m) => `pub(super) ${stripPub(m)}`)
      .replace(/^(?:pub(?:\([^)]*\))?\s+)?(struct|enum|type)\s+(\w+)/gm, (_m, kw, name) => `pub(super) ${kw} ${name}`)
      .replace(/^(?:pub(?:\([^)]*\))?\s+)?const\s+(\w+)/gm, (_m, name) => `pub(super) const ${name}`)
    text = widenInherentImpls(text)
    text = widenTopLevelStructFields(text)
  }
  return text
}

// 路径敏感用例：文件被搬到 tests/ 子目录后，include_str! 的相对路径会指向新位置，
// 源码扫描类断言会静默失效或直接编译不过。这类块必须留给手工处理。
const pathSensitive = block.rawBody.some((l) => /include_str!|include_bytes!/.test(l))

const totalTests = finalGroups.reduce((a, g) => a + g.tests.length, 0)
if (pathSensitive && !hasFlag('allow-path-sensitive')) {
  console.error('该测试块含 include_str!/include_bytes!（源码扫描类断言）——搬迁会改写其相对路径语义，请手工处理')
  process.exit(5)
}
if (totalTests === 0 && !hasFlag('allow-empty')) {
  const mods = finalGroups.flatMap((g) => g.helpers).filter((c) => c.kind === 'mod').length
  const items = finalGroups.flatMap((g) => g.helpers).length
  console.error(
    `该测试模块无顶层用例（用例嵌在内联 mod 里；脚手架 ${items} 项 / 内联 mod ${mods} 个）——` +
      '自动拆分只会切碎脚手架，请手工处理或加 --allow-empty 强制',
  )
  process.exit(4)
}

const plan = finalGroups.map((g) => ({ name: g.name, section: g.section, tests: g.tests.length, lines: g.tests.reduce((a, c) => a + c.lines.length, 0) }))

console.log(`\n源文件：${fileArg}`)
console.log(`目标：  ${target}    测试模块：${modName}`)
console.log(`原单块：${block.endLine - block.startLine + 1} 行 / ${finalGroups.reduce((a, g) => a + g.tests.length, 0)} 个用例`)
console.log(`拆为 ${plan.length} 个文件：`)
for (const p of plan) console.log(`   ${p.name.padEnd(28)} ${String(p.tests).padStart(3)} 用例  ${String(p.lines).padStart(4)} 行   ← ${p.section}`)
if (target === 'public') console.log(`\n公共目标：每个 tests/*.rs 是独立 crate，入口 use ${usePath}::*;`)
if (allHelpers.length) {
  const dest = target === 'public' ? '<entry>' : useScaffoldFile ? `tests/${scaffoldName}.rs` : '<entry>'
  console.log(`共享脚手架 ${helperNames.length} 项 → ${dest}：${helperNames.join(', ')}`)
}
console.log()

if (dry) process.exit(0)

// ---- 写入前的守门：用例数必须守恒 ----
// 静默少一个 #[test] = 静默丢一块覆盖率，这比拆不开严重得多。
// 写入前先算出「产物里的用例数」，对不上就**不写盘**并报错。
const countTests = (text) => (text.match(/^\s*#\[(?:tokio::)?test(?:\s*\([^)]*\))?\]\s*$/gm) || []).length
const BEFORE = countTests(block.rawBody.join('\n'))
const AFTER = finalGroups.reduce((a, g) => a + countTests(g.tests.flatMap(bodyLines).join('\n')), 0)
if (BEFORE !== AFTER) {
  console.error(`用例数不守恒：原块 ${BEFORE} 个 #[test]，分组后 ${AFTER} 个——已中止（未写盘）`)
  console.error('常见原因：属性行被空行/注释切走，或用例嵌在内联 mod 里')
  process.exit(6)
}
if (BEFORE === 0) {
  console.error('原块内没有 #[test]——已中止（未写盘）')
  process.exit(6)
}

// ==================== 落盘 ====================

// crate 根文件（lib.rs / main.rs）：内联 `mod tests` 的子模块解析到 `<crate>/tests/`，
// 不带 lib/main 这一层；其余文件解析到 `<dir>/<modStem>/tests/`。
const finalOutDir = target === 'private'
  ? (fileBase === 'lib' || fileBase === 'main'
      ? join(dirname(FILE), 'tests')
      : join(dirname(fileBase === 'mod' ? dirname(FILE) : FILE), modStem, 'tests'))
  : join(FILE.split('/src/')[0], 'tests')
mkdirSync(finalOutDir, { recursive: true })

for (const g of finalGroups) {
  const p = join(finalOutDir, `${g.name}.rs`)
  writeFileSync(p, renderFile(g, { scaffold: useScaffoldFile }))
  console.log(`写 ${relative(ROOT, p)}`)
}
if (useScaffoldFile) {
  const p = join(finalOutDir, `${scaffoldName}.rs`)
  writeFileSync(p, renderScaffold())
  console.log(`写 ${relative(ROOT, p)}`)
}

// 回写源文件
const entryUses = commonUses.length
  ? commonUses
      .map((u) => u.split('\n').map((l) => (l.trim() ? `    ${l.trim()}` : l)).join('\n'))
      .join('\n')
  : ''

const modPath = crateModulePath(FILE).replace('crate::', '')
const outRel = relative(dirname(FILE), finalOutDir)
// 公共目标：全部用例已进 tests/，crate 内不再留任何测试模块（import 也随之消失）
const entryIsEmpty = target === 'public' && allHelpers.length === 0
const tailLines = entryIsEmpty
  ? [
      '// ==================== Tests ====================',
      '',
      `// 单元测试已全部迁至 \`${outRel}/\`（集成测试，只经 crate 公开 API 验证契约）。`,
    ]
  : [
      '// ==================== Tests ====================',
      '',
      `// 用例按功能拆至 \`${outRel}/\`（本内联模块的子模块路径由 rustc`,
      `// 自动解析到该目录；模块树 \`${modPath}::tests::<文件>\` 与内联形态等价，私有项可见性不受影响）。`,
      '#[cfg(test)]',
      'mod tests {',
      ...(entryUses ? [entryUses] : []),
      ...(allHelpers.length && !useScaffoldFile
        ? [
            '    // 跨分组共享的测试脚手架（子模块经 `use super::*` 可见）',
            '',
            ...dedent(allHelpers.flatMap(bodyLines)).map((l) => (l ? `    ${l}` : l)),
          ]
        : []),
      ...(useScaffoldFile ? [`    mod ${scaffoldName};`] : []),
      ...finalGroups.map((g) => `    mod ${g.name};`),
      '}',
    ]

const lines = src.split('\n')
lines.splice(block.startLine, block.endLine - block.startLine + 1, ...tailLines)
writeFileSync(FILE, lines.join('\n'))
console.log(`改写 ${fileArg}`)