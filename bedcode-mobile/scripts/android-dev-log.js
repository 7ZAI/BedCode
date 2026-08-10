/**
 * tauri android dev 一键日志落盘版（电脑端）
 *
 * 原理：移动端进程运行在 Android 设备上，Rust 代码无法直接写电脑磁盘，
 * 但 `tauri android dev` 的 Tauri CLI 会把移动端 logcat 实时转发到电脑
 * 控制台 —— 本脚本把控制台输出同时写一份到电脑端日志文件（按天轮转），
 * 等价于 `npm run tauri:android:dev 2>&1 | tee ...`，跨平台（Windows cmd 无 tee）。
 *
 * 用法：npm run tauri:android:dev:log
 * 日志目录：bedcode-mobile/.dev-logs/android-dev.YYYY-MM-DD.log（UTC 日期，与设备日志一致）
 */
import { spawn } from 'node:child_process'
import { createWriteStream, mkdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { StringDecoder } from 'node:string_decoder'

const __dirname = dirname(fileURLToPath(import.meta.url))
const LOG_DIR = join(__dirname, '..', '.dev-logs')
mkdirSync(LOG_DIR, { recursive: true })

// 按天轮转（UTC 日期与设备端 runtime.*.log 的日期线对齐）
const logFile = join(LOG_DIR, `android-dev.${new Date().toISOString().slice(0, 10)}.log`)
// flags 'w'：每次启动清空当天日志文件，保证一次 dev 会话从头开始可查（跨天仍按 UTC 轮转新文件）
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

const child = spawn('npm', ['run', 'tauri:android:dev'], {
  stdio: ['inherit', 'pipe', 'pipe'],
  shell: process.platform === 'win32',
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

// Ctrl+C / 终止信号：等缓冲区落盘再退出，避免截断尾部日志
// （flags 'w' 下尾部丢失 + 下次启动覆盖当天文件 = 该段日志永久不可查）
for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => {
    stream.end(() => process.exit(0))
  })
}
