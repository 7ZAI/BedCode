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

const __dirname = dirname(fileURLToPath(import.meta.url))
const LOG_DIR = join(__dirname, '..', '.dev-logs')
mkdirSync(LOG_DIR, { recursive: true })

// 按天轮转（UTC 日期与设备端 runtime.*.log 的日期线对齐）
const logFile = join(LOG_DIR, `android-dev.${new Date().toISOString().slice(0, 10)}.log`)
const stream = createWriteStream(logFile, { flags: 'a' })

// 去掉 ANSI 颜色码：控制台保留彩色，文件存纯文本便于 grep/cat
const stripAnsi = (s) => s.replace(/\x1b\[[0-9;]*m/g, '')

console.log(`[dev-log] 电脑端日志落盘: ${logFile}`)

const child = spawn('npm', ['run', 'tauri:android:dev'], {
  stdio: ['inherit', 'pipe', 'pipe'],
  shell: process.platform === 'win32',
})

for (const fd of ['stdout', 'stderr']) {
  child[fd]?.on('data', (chunk) => {
    process[fd].write(chunk) // 保持控制台实时输出
    stream.write(stripAnsi(chunk.toString()))
  })
}

child.on('error', (err) => {
  console.error('[dev-log] 启动失败:', err)
  stream.end()
  process.exit(1)
})

child.on('exit', (code) => {
  stream.end()
  console.log(`[dev-log] 已退出（code=${code}），日志保留在 ${logFile}`)
  process.exit(code ?? 0)
})
