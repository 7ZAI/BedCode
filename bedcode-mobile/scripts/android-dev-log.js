/**
 * tauri android dev 一键日志落盘版（电脑端）
 *
 * 原理：移动端进程运行在 Android 设备上，Rust 代码无法直接写电脑磁盘，
 * 但 `tauri android dev` 的 Tauri CLI 会把移动端 logcat 实时转发到电脑
 * 控制台 —— 本脚本把控制台输出同时写一份到电脑端日志文件（按天轮转），
 * 等价于 `pnpm run tauri:android:dev 2>&1 | tee ...`，跨平台（Windows cmd 无 tee）。
 *
 * 用法：pnpm run tauri:android:dev:log
 * 日志目录：bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log（本地日期，与设备日志日期线一致）
 *
 * 非业务日志过滤（控制台实时输出与落盘文件同一套过滤，保留行双写）：
 *   - 设备侧 wasmtime/cranelift JIT 编译内部 debug（cranelift_codegen::* 等，
 *     插件加载瞬间洪水刷屏，实测单日 ~12 万行日志里占 ~9 成）、mdns_sd:: 库内部 debug
 *   - 设备侧 Android/MIUI 框架噪音 tag：按「本方 tag 白名单」判定——BedCode* 前缀、
 *     Tauri/Console、System.out、RustStdoutStderr、PluginAssetExtractor、
 *     ForegroundService，以及各崩溃关键 tag（AndroidRuntime / libc / DEBUG）；
 *     其余框架 tag 一律过滤，新设备新框架 tag 自动滤除，无需维护列表
 *   - 主机侧 tauri CLI -v 的 Debug 行与 neli 路由表 dump、Gradle 任务/配置进展、
 *     Vite 与插件 watch 重建的重复进展行（构建/编译错误不匹配这些行，天然保留）
 *   业务日志（bedcode_lib::*、bedcode_peer_net::*、[plugin:xxx]、前端 relay、
 *   Kotlin 侧 BedCode-*）与链路排障日志（reqwest::connect 等）全部保留。
 *   完整关闭过滤（行为同旧版全量；控制台不再带 ANSI 颜色，统一纯文本管线）：
 *   BEDCODE_LOG_NO_FILTER=1 pnpm run tauri:android:dev:log
 */
import { spawn } from 'node:child_process'
import { createWriteStream, mkdirSync, readdirSync, statSync, unlinkSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { StringDecoder } from 'node:string_decoder'

const __dirname = dirname(fileURLToPath(import.meta.url))
const LOG_DIR = join(__dirname, '..', '.dev-logs')
mkdirSync(LOG_DIR, { recursive: true })

// 保留天数：dev 日志按天轮转但不清理，长期开发 .dev-logs 会无限增长（单日
// 会话可达数十 MB）；每次启动清理超期旧文件（桌面端 max_files 同思路）
const RETENTION_DAYS = 14
function cleanupOldLogs() {
  const cutoff = Date.now() - RETENTION_DAYS * 24 * 60 * 60 * 1000
  for (const name of readdirSync(LOG_DIR)) {
    if (!name.startsWith('android-dev.') || !name.endsWith('.log')) continue
    const p = join(LOG_DIR, name)
    try {
      if (statSync(p).isFile() && statSync(p).mtimeMs < cutoff) {
        unlinkSync(p)
        console.log(`[dev-log] 清理过期日志（保留 ${RETENTION_DAYS} 天）: ${name}`)
      }
    } catch {
      // 单文件清理失败不阻断启动
    }
  }
}
cleanupOldLogs()

// 按天轮转。注意用本地日期：toISOString() 是 UTC，UTC+8 凌晨 0–7 点会
// 把日志落进「昨天」的文件（设备 logcat 时间是本地时间，文件却少一天）。
// 与桌面端 runtime.*.log（tracing_appender 用 UTC 命名）不同，此处以设备
// 日志的本地日期线对齐，凌晨跨天时会按本地日期换新文件。
const now = new Date()
const localDate = [
  now.getFullYear(),
  String(now.getMonth() + 1).padStart(2, '0'),
  String(now.getDate()).padStart(2, '0'),
].join('-')
const logFile = join(LOG_DIR, `android-dev.${localDate}.log`)
// flags 'w'：每次启动清空当天日志文件，保证一次 dev 会话从头开始可查（跨天仍按本地日期轮转新文件）
const stream = createWriteStream(logFile, { flags: 'w' })

// stdout / stderr 是两条独立字节流，需各自维护解码状态，
// 否则多字节 UTF-8 字符在 chunk 边界被截断会产生替换符（U+FFFD）损坏日志
const decoders = { stdout: new StringDecoder('utf8'), stderr: new StringDecoder('utf8') }

// 去掉 ANSI 转义序列：控制台保留彩色，文件存纯文本便于 grep/cat
// - CSI 序列：颜色 \x1b[38;5;123m、清屏 \x1b[2J、光标 \x1b[1A、行擦除 \x1b[2K、光标显隐 \x1b[?25l 等
// - OSC 序列：如终端标题 \x1b]0;...\x07
// - 进度条覆盖用的 \r（纯文本里覆盖不生效，只会把多次进度拼成长行）
const stripAnsi = (s) =>
  s
    .replace(/\x1b\[[0-9;?]*[ -\/]*[@-~]/g, '')
    .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
    .replace(/\r/g, '')

// ==================== 非业务日志过滤（控制台 + 落盘同一套过滤） ====================
// 规则详解见文件头注释；BEDCODE_LOG_NO_FILTER=1 时完全关闭过滤（全量，纯文本无 ANSI）。
const NO_LOG_FILTER = process.env.BEDCODE_LOG_NO_FILTER === '1'

// —— 行内容模式判定丢弃（覆盖 logcat 的 BedCode tag 内部与主机侧输出）——
const DROP_PATTERNS = [
  // wasmtime/cranelift JIT 编译内部（占单日 dev 日志 ~9 成，插件加载时洪水）
  /cranelift_codegen::/,
  /wasmtime_internal_cranelift::/,
  /wasmtime_cranelift::/,
  /wasmtime::runtime::code_memory/,
  // mdns_sd crate 内部 debug（业务发现日志在 bedcode_lib::mdns::* / bedcode_peer_net::*）
  /mdns_sd::/,
  // 主机侧 tauri CLI -v 的 Debug 行（结构 dump / 文件监视器）与 neli 路由表 dump
  /\[neli::socket\]/,
  /^Nlmsghdr /,
  /^\s*Debug \[/,
  // Tauri/Console relay 中 vite HMR 连接状态（frontendLogger 业务日志不受影响）
  /Msg: \[vite\] (connecting\.{3}|connected\.)/,
  // 主机侧 Gradle 任务/配置进展（FAILURE / What went wrong / e: 等错误行不受影响）
  /^:[a-zA-Z0-9_-]+:/,
  /^> Task :/,
  /^> Configure project /,
  /^Resolve mutations for :/,
  /^Skipping task ':/,
  /^Tasks to be executed: \[task '/,
  /^work action (resolve|Parameters) /,
  /^Now considering \[/,
  /^ *Simple merging task$/,
  /Caching has been disabled for the task/,
  / Caching disabled for task ':/,
  /Task has not declared any outputs/,
  /This task renders reported diagnostics/,
  /^Task ':.+ is not up-to-date/,
  /^Task name matched '/,
  /^Using Kotlin Gradle Plugin gradle\d+ variant/,
  /^Using default execution profile/,
  /^Using \d+ worker leases/,
  /Resolved plugin \[id: '/,
  /^ *Not worth caching/,
  /^No compile result for :/,
  /^Build cache key for /,
  /^Custom actions are attached to task ':/,
  /^DexingNoClasspathTransform /,
  /^ClassesDirToClassesTransform /,
  /^kotlin scripting plugin: created the scripting discovery configuration/,
  /^Starting \d+(st|nd|rd|th) build in daemon /,
  /^Evaluating project ':/,
  /^Projects loaded\./,
  /^Included projects: \[/,
  /^All projects evaluated\./,
  /^Selected primary task '/,
  /^The client will now receive all logging from the daemon/,
  /^Successfully started process 'command '/,
  /^Could not execute \[report metric /,
  /^File system watching is active/,
  /file or directory '.*', not found/,
  // 主机侧 Vite / 插件 watch 重建的重复进展与告警
  /^\(!\) Your Vite config uses features/,
  /^Set `VITE_CONFIG_NATIVE_IGNORE_WARNING=true` to suppress this warning\.$/,
  /^manually calling optimizeDeps is deprecated/,
  /^\(client\) Hash is consistent\./,
  / modules transformed\.$/,
  /^(transform(ing)?|rendering chunks|computing gzip size|watching for file changes|build started)\.\.\.$/,
  /^built in \d+(ms|s)\.$/,
  /^vite v\d+\.\d+\.\d+ building client environment/,
  // 插件 watch 的重复 banner（「产物已复制 / 自动复制到」行保留）
  /^\[bedcode-plugin\] =+/,
  /^\[bedcode-plugin\] Ctrl\+C 退出/,
  /^\[bedcode-plugin\] 修改插件前端源码后自动重建/,
]

// —— logcat 行按 tag 白名单判定（设备侧；其余框架 tag 一律过滤）——
const LOGCAT_LINE = /^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d+\s+\d+\s+\d+\s+[VDIWEF]\s+/
function logcatTag(line) {
  const m = LOGCAT_LINE.exec(line)
  if (!m) return null
  const rest = line.slice(m[0].length)
  const colon = rest.indexOf(':')
  if (colon <= 0) return null
  return rest.slice(0, colon).trim()
}
// 本方标签族：BedCode 前缀覆盖 Rust tracing 主 tag 与 Kotlin 侧 BedCode-* 系列；
// 另保留各崩溃关键框架 tag（Java crash FATAL / 原生 Fatal signal / tombstone）。
// 新设备出现的新框架 tag 无需维护——不在白名单即自动滤除。
const KEEP_LOGCAT_TAGS = new Set([
  'Tauri/Console', // 前端 console relay（frontendLogger 业务日志）
  'System.out', // Kotlin println（EdgeToEdge 等）
  'RustStdoutStderr', // Rust println 透传
  'PluginAssetExtractor', // 插件资源解压（Kotlin 侧）
  'ForegroundService', // 前台服务（Kotlin 侧）
  'AndroidRuntime', 'libc', 'DEBUG', // 崩溃链路：Java FATAL / 原生信号 / tombstone
])
function isKeepLogcatTag(tag) {
  if (tag.startsWith('BedCode')) return true
  return KEEP_LOGCAT_TAGS.has(tag)
}

// —— 过滤执行：行缓冲（chunk 可能跨行边界，必须按完整行判定）+ 续行规则 + 统计 ——
// 完整行统一判定：保留行同时写控制台与落盘文件（同一套规则，避免控制台仍刷
// cranelift 等洪水噪音）；BEDCODE_LOG_NO_FILTER=1 时全量不过滤。
const isContinuation = (line) => /^\s+\S/.test(line) // 缩继续行（栈帧 \tat / gradle 详情等）
let prevLineDropped = false
const filterStats = { total: 0, kept: 0, dropped: 0, droppedContinuation: 0 }

function isNoiseLine(line) {
  if (NO_LOG_FILTER) return false
  if (DROP_PATTERNS.some((re) => re.test(line))) return true
  const tag = logcatTag(line)
  if (tag) return !isKeepLogcatTag(tag)
  return prevLineDropped && isContinuation(line)
}

const lineBuf = { stdout: '', stderr: '' }
function flushLine(line, fd) {
  if (line === '') return
  filterStats.total += 1
  const noise = isNoiseLine(line)
  prevLineDropped = noise
  if (noise) {
    filterStats.dropped += 1
    if (isContinuation(line)) filterStats.droppedContinuation += 1
    return
  }
  filterStats.kept += 1
  process[fd].write(line + '\n') // 控制台（与来源 fd 一致）
  stream.write(line + '\n') // 落盘
}
function flushPendingLines(fd) {
  if (lineBuf[fd]) {
    flushLine(lineBuf[fd], fd)
    lineBuf[fd] = ''
  }
}
function printFilterStats() {
  if (!filterStats.total || NO_LOG_FILTER) return
  const pct = ((filterStats.kept / filterStats.total) * 100).toFixed(1)
  console.log(
    `[dev-log] 日志过滤统计：处理 ${filterStats.total} 行 → 保留 ${filterStats.kept}（${pct}%），` +
      `过滤 ${filterStats.dropped} 行（其中缩继续行 ${filterStats.droppedContinuation}）; ` +
      'BEDCODE_LOG_NO_FILTER=1 可关闭过滤看全量',
  )
}

console.log(`[dev-log] 电脑端日志落盘: ${logFile}`)
console.log('[dev-log] 已启用 verbose（BEDCODE_DEV_VERBOSE=1）：logcat 转发含 Debug 级别（Rust debug! 日志可见）')
if (!NO_LOG_FILTER) {
  console.log('[dev-log] 已启用非业务日志过滤（控制台 + 落盘同规则：wasmtime/cranelift·框架噪音·构建进展过滤，业务与链路日志保留；退出时打印统计；BEDCODE_LOG_NO_FILTER=1 可关闭）')
}

const IS_WIN = process.platform === 'win32'
// BEDCODE_DEV_VERBOSE=1 → dev-run.js 给 turi android dev 追加 -v：CLI 的 logcat
// 转发默认 Polite→Info 过滤，Rust debug!（D 级，如 terminal_link 收帧统计）
// 不开 -v 永远进不了控制台与本落盘文件。:dev:log 模式定位就是排查落盘，默认开启
const child = spawn(IS_WIN ? 'pnpm.cmd' : 'pnpm', ['run', 'tauri:android:dev'], {
  stdio: ['inherit', 'pipe', 'pipe'],
  shell: IS_WIN,
  env: { ...process.env, BEDCODE_DEV_VERBOSE: '1' },
  // POSIX：detached 让子进程自成进程组，信号处理可对整个组（含 dev-run.js 及其
  // 全部 watch/宿主子树）一次性回收；Ctrl+C 不再直送子进程，由下方 handler 转发
  detached: !IS_WIN,
})

for (const fd of ['stdout', 'stderr']) {
  child[fd]?.on('data', (chunk) => {
    lineBuf[fd] += stripAnsi(decoders[fd].write(chunk))
    const lines = lineBuf[fd].split('\n')
    lineBuf[fd] = lines.pop() ?? '' // 末段可能是半行，留到下一个 chunk 或进程退出时再判
    lines.forEach((line) => flushLine(line, fd))
  })
}

child.on('error', (err) => {
  console.error('[dev-log] 启动失败:', err)
  flushPendingLines('stdout')
  flushPendingLines('stderr')
  printFilterStats()
  // 等缓冲区落盘再退出，避免截断尾部日志
  stream.end(() => process.exit(1))
})

child.on('exit', (code) => {
  console.log(`[dev-log] 已退出（code=${code}），日志保留在 ${logFile}`)
  flushPendingLines('stdout')
  flushPendingLines('stderr')
  printFilterStats()
  // 等缓冲区落盘再退出，避免截断尾部日志
  stream.end(() => process.exit(code ?? 0))
})

// Ctrl+C / 终止信号 / 关闭终端标签页：先回收整棵子进程树（dev-log 退出了子进程
// 不会跟着退，历史上残留 vite/插件 watch），再等缓冲区落盘退出，避免截断尾部日志
// （flags 'w' 下尾部丢失 + 下次启动覆盖当天文件 = 该段日志永久不可查）
let signalHandled = false
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(sig, () => {
    if (signalHandled) return
    signalHandled = true
    if (child.pid) {
      try {
        process.kill(-child.pid, 'SIGTERM')
      } catch {
        try {
          child.kill('SIGTERM')
        } catch {
          // 已退出，忽略
        }
      }
    }
    // 子进程树收到 SIGTERM 自行回收；这里给短宽限让 dev-run.js 完成日志冲刷
    setTimeout(() => {
      flushPendingLines('stdout')
      flushPendingLines('stderr')
      printFilterStats()
      stream.end(() => process.exit(0))
    }, 500).unref()
  })
}
