#!/usr/bin/env node
/**
 * 终端中文字体子集生成器（一次性；产物已入库，平时无需运行）
 *
 * 为什么需要：终端行尾「凹凸」/ 色块摆动的真因是 CJK 回退字体 advance ≠ 2× 格宽
 * （见 src/styles/terminal.css「行尾软裁切」注释）——系统等宽字体给拉丁 advance，
 * 中文字形落到系统比例 CJK 字体（1em），1em ≠ 2×0.6em，误差逐字符累积。
 * 唯一根治是「CJK 严格 2 格的等宽字体」：更纱黑体 Sarasa Mono SC（拉丁 0.5em +
 * 思源黑体 CJK 1em，box-drawing 0.5em，OFL-1.1 可随包分发）。
 *
 * 为什么不直接随包完整字体：完整 SarasaMonoSC-Regular.ttf 约 14MB，对 APK 不可接受。
 * 本脚本按「终端实际会出现的字符集」做子集（GB2312 全集 6763 汉字 + 制表符 + 块元素 +
 * 标点 + 数学/箭头/技术符号 + 假名 + 全半角），产物约 1~2MB woff2。
 * 子集外的汉字（繁体、生僻字）落回系统 CJK 字体：其 advance 恒为 1em = 2×0.5em 格宽，
 * 仍然对齐，不破坏「CJK = 2 格」这条不变量。
 *
 * 用法（需要先安装子集工具，见 devDependencies.subset-font）：
 *   1. 从 https://github.com/be5invis/Sarasa-Gothic/releases 取 SarasaMonoSC-TTF-Unhinted
 *      的 SarasaMonoSC-Regular.ttf（Unhinted：WebView 走 FreeType 灰度渲染，hinting
 *      的整数化 advance 反而会引入舍入偏差）
 *   2. cd bedcode-mobile && node scripts/build-terminal-font.mjs <路径>/SarasaMonoSC-Regular.ttf
 *
 * 产物：src/assets/fonts/SarasaMonoSC-Terminal-Regular.woff2
 * 许可：OFL-1.1（Copyright (c) 2015-2025 Belleve Invis 等；其 CJK 部分来自 Adobe
 * Source Han Sans，保留字体名 'Source'——本子集的主字体名「Sarasa Mono SC」不含
 * 该保留名，OFL 第 3 条满足）。许可证全文随字体入库于同目录 LICENSE-Sarasa-Gothic.txt
 * （OFL 第 2 条：分发时须附版权声明与许可证）。
 */
import { readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import subsetFont from 'subset-font'

const HERE = dirname(fileURLToPath(import.meta.url))
const OUT_PATH = resolve(HERE, '../src/assets/fonts/SarasaMonoSC-Terminal-Regular.woff2')

/**
 * 字符集：逐段声明「为什么这段在终端里会出现」，不写无出处的宽区间。
 * 形如 [起, 止]（含端点）。制表符 / 块元素 / CJK 标点是 TUI 画框与对齐的硬需求，
 * 缺一段就会让「行尾对齐」在某些输出上重新失准。
 */
const CODEPOINT_RANGES = [
  [0x0020, 0x007e], // ASCII 可打印（命令、路径、diff）
  [0x00a0, 0x00ff], // Latin-1 补充（é ü ñ ± × ÷）
  [0x0100, 0x017f], // Latin Extended-A（部分构建日志的人名/地名）
  [0x0370, 0x03ff], // 希腊字母（数学/科学输出）
  [0x0400, 0x04ff], // 西里尔（编译日志、git 作者名）
  [0x2000, 0x206f], // 通用标点（— – “ ” ‘ ’ … • ‰）
  [0x20a0, 0x20bf], // 货币符号
  [0x2100, 0x214f], // 字母类符号（™ © ® ℓ № ⌐）
  [0x2190, 0x21ff], // 箭头
  [0x2200, 0x22ff], // 数学运算符（∑ √ ≈ ≤ ≥ ⇒）
  [0x2300, 0x23ff], // 技术符号（⌘ ⌥ ⇧ ⏎ ⌫ ⏏，TUI 快捷键提示常用）
  [0x2500, 0x257f], // 制表符 box drawing（╔ ═ ╗ ║ ╚ ╝ ─ │）★ TUI 画框硬需求
  [0x2580, 0x259f], // 块元素（█ ▀ ▄ ▌ ▐）★ 进度条/填充硬需求
  [0x25a0, 0x25ff], // 几何图形（● ■ ▲ ▼ ◆ ○ ◇）
  [0x2600, 0x27bf], // 杂项符号 + dingbats（★ ✓ ✗ ⚡ → 无 emoji，emoji 走系统字体）
  [0x2b00, 0x2bff], // 杂项符号与箭头（⬆ ⬇ ⭐）
  [0x3000, 0x303f], // CJK 符号与标点（。、「」《》〜）★ 中文排版硬需求
  [0x3040, 0x30ff], // 假名（日文输出）
  [0xff00, 0xffef], // 全角/半角形式（Ａ１、ｱｲｳ）
]

/**
 * GB2312 全集 6763 汉字 + 682 符号：GBK 双字节区间 0xA1A1–0xF7FE 逐字节解码枚举
 * （Node 内置 TextDecoder('gb18030') 覆盖 GB2312 全集）。
 * 选 GB2312 而非「通用规范汉字表一级字表」：终端里出现的是任意中文文本（日志、
 * 注释、AI 输出），6763 字才是「简体中文文本基本不会缺字」的集合。
 */
function gb2312Chars() {
  const decoder = new TextDecoder('gb18030')
  const out = []
  for (let hi = 0xa1; hi <= 0xf7; hi++) {
    for (let lo = 0xa1; lo <= 0xfe; lo++) {
      const ch = decoder.decode(Uint8Array.from([hi, lo]))
      // 非法双字节被替换为 U+FFFD，跳过
      if (ch === '�' || ch.length === 0) continue
      out.push(ch)
    }
  }
  return out
}

function buildCharset() {
  const chars = new Set()
  for (const [start, end] of CODEPOINT_RANGES) {
    for (let cp = start; cp <= end; cp++) chars.add(String.fromCodePoint(cp))
  }
  for (const ch of gb2312Chars()) chars.add(ch)
  // 控制字符不制作为字形，显式剔除（GB2312 枚举里含 U+3000 之外的少量不可见字符）
  for (let cp = 0; cp < 0x20; cp++) chars.delete(String.fromCodePoint(cp))
  return [...chars].sort().join('')
}

/**
 * 保留 name 表全部条目：OFL 第 2 条要求「机器可读元数据字段」携带版权与许可，
 * 随包二进制里带着 copyright / license URL 比只放一个 LICENSE 文本更稳
 * （字体被单独提取时仍可溯源）。
 */
const PRESERVE_NAME_IDS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 16, 17]

const inputPath = process.argv[2]
if (!inputPath) {
  console.error('用法: node scripts/build-terminal-font.mjs <SarasaMonoSC-Regular.ttf>')
  process.exit(1)
}

const text = buildCharset()
const source = readFileSync(resolve(inputPath))
console.log(`字符集：${[...text].length} 个码位（源字体 ${(statSync(resolve(inputPath)).size / 1e6).toFixed(1)}MB）`)

const subset = await subsetFont(source, text, {
  targetFormat: 'woff2',
  preserveNameIds: PRESERVE_NAME_IDS,
})

mkdirSync(dirname(OUT_PATH), { recursive: true })
writeFileSync(OUT_PATH, subset)
console.log(`产物：${OUT_PATH}（${(subset.length / 1024).toFixed(0)}KB）`)
