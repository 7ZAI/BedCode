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

console.log(`[dev-log] 电脑端日志落盘: ${logFile}`)

const IS_WIN = process.platform === 'win32'
const child = spawn(IS_WIN ? 'pnpm.cmd' : 'pnpm', ['run', 'tauri:android:dev'], {
  stdio: ['inherit', 'pipe', 'pipe'],
  shell: IS_WIN,
  // POSIX：detached 让子进程自成进程组，信号处理可对整个组（含 dev-run.js 及其
  // 全部 watch/宿主子树）一次性回收；Ctrl+C 不再直送子进程，由下方 handler 转发
  detached: !IS_WIN,
})

for (const fd of ['stdout', 'stderr']) {
  child[fd]?.on('data', (chunk) => {
    process[fd].write(chunk) // 保持控制台实时输出
    stream.write(stripAnsi(decoders[fd].write(chunk)))
  })
}

child.on('error', (err) => {
  console.error('[dev-log] 启动失败:', err)
  // 等缓冲区落盘再退出，避免截断尾部日志
  stream.end(() => process.exit(1))
})

child.on('exit', (code) => {
  console.log(`[dev-log] 已退出（code=${code}），日志保留在 ${logFile}`)
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
    setTimeout(() => stream.end(() => process.exit(0)), 500).unref()
  })
}
