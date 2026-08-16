#!/usr/bin/env node
/**
 * Dev Runner — 并行启动插件前端 watch 构建与宿主 dev 命令
 *
 * `npm run tauri:dev` 一条命令即完成：
 *   - 各插件前端 watch：改源码自动重建 + 复制产物（配合宿主 PluginDevWatcher 触发前端热重载）
 *   - 宿主 dev 进程（tauri dev）
 * 任一子进程退出（Ctrl+C / 宿主崩溃）时统一回收全部。
 *
 * 用法：
 *   node scripts/dev-run.js              # 默认：三个插件 watch + tauri dev
 *   node scripts/dev-run.js --host-cmd "<命令>"   # 覆盖宿主命令（按空格拆分）
 */

import { spawn, execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import net from 'node:net'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const IS_WIN = process.platform === 'win32'

// ==================== 端口预检 ====================

/**
 * 宿主 beforeDevCommand（vite dev server）端口预检。
 *
 * tauri dev 启动时自动运行 beforeDevCommand（npm run dev → vite），若端口被残留进程
 * 占用（常见于上次 dev 会话 Ctrl+C 后 vite 未退出），宿主启动必失败且表现像卡死
 * （无提示等待 → “Port is already in use” 报错）。此处提前检测并给出明确指引。
 */
async function precheckDevPort() {
  let devUrl = null
  try {
    const conf = JSON.parse(readFileSync(resolve(ROOT, 'src-tauri/tauri.conf.json'), 'utf-8'))
    devUrl = conf?.build?.devUrl
  } catch {
    return // 读取配置失败不阻塞（预检是防御性的）
  }
  if (!devUrl) return

  let port = 0
  try {
    port = Number(new URL(devUrl).port)
  } catch {
    return
  }
  if (!port) return

  // 尝试监听：成功 = 空闲；失败（EADDRINUSE）= 被占用。
  // 占用者监听 0.0.0.0 时绑定 127.0.0.1 同样会冲突，覆盖两种监听方式
  const inUse = await new Promise((resolve_) => {
    const srv = net.createServer()
    srv.once('error', () => resolve_(true))
    srv.once('listening', () => srv.close(() => resolve_(false)))
    srv.listen(port, '127.0.0.1')
  })
  if (!inUse) return

  console.error(`[dev-run] ⚠ 端口 ${port}（${devUrl}，宿主 beforeDevCommand vite dev server）已被占用`)
  console.error('[dev-run]   通常是上次 dev 会话残留的 vite 进程，宿主启动必失败。请先结束占用进程：')
  console.error(`[dev-run]   netstat -ano | findstr :${port}   然后   taskkill /F /PID <pid>`)
  process.exit(1)
}

/**
 * HMR 端口预检（仅局域网模式）。
 *
 * 设置了 TAURI_DEV_HOST 时 vite 监听 0.0.0.0:1420，HMR WebSocket 走独立端口 1421
 * （见 vite.config.ts server.hmr）。若 1421 被其他进程/残留 dev 会话占用，页面可正常
 * 加载（1420 通）但热更新静默失效——最隐蔽的“改代码不刷新”。仅检测不阻断：
 * 占用者可能是另一个正在运行的 dev 会话（本就该共存），此时提示排查方向即可。
 */
async function precheckHmrPort() {
  if (!process.env.TAURI_DEV_HOST) return
  const port = 1421

  const inUse = await new Promise((resolve_) => {
    const srv = net.createServer()
    srv.once('error', () => resolve_(true))
    srv.once('listening', () => srv.close(() => resolve_(false)))
    srv.listen(port, '127.0.0.1')
  })
  if (!inUse) return

  console.warn(`[dev-run] ⚠ 检测到 TAURI_DEV_HOST=${process.env.TAURI_DEV_HOST}（局域网模式），但 HMR 端口 ${port} 已被占用`)
  console.warn('[dev-run]   页面可正常加载但热更新会失效（改代码不刷新）。占用者通常是残留 dev 会话：')
  console.warn(`[dev-run]   netstat -ano | findstr :${port}   然后   taskkill /F /PID <pid>`)
  console.warn('[dev-run]   若不再需要局域网调试，删除环境变量后重启 dev 即回到本机模式（HMR 同端口 1420）')
}

// npm-cli.js 绝对路径：优先取 npm 注入的 npm_execpath（任何安装布局下都正确），
// 回退到 Windows Node 安装器标准布局（node.exe 与 node_modules/npm 同目录）；
// Linux/macOS 的 npm 在系统目录（/usr/lib/node_modules/npm 等），与 node 二进制
// 不同目录，故不能只用回退路径
const NPM_CLI =
  process.env.npm_execpath ??
  resolve(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')

// ==================== 平台配置 ====================

/** 插件 watch 启动项（dir 相对仓库根，args 在插件目录内执行） */
const PLUGIN_WATCH_CMDS = [
  { dir: 'plugins/ai-chatbox', args: ['scripts/build.js', '--watch'] },
  { dir: 'plugins/auto-task', args: ['scripts/build.js', '--watch'] },
  { dir: 'plugins/file-transfer', args: ['scripts/build.js', '--watch'] },
]

/** 宿主 dev 命令（可用 --host-cmd 覆盖） */
const DEFAULT_HOST_CMD = [process.execPath, [NPM_CLI, 'run', 'tauri', '--', 'dev']]

// ==================== 进程管理 ====================

const children = []
let shuttingDown = false

function start(cmd, args, cwd) {
  const child = spawn(cmd, args, { cwd, stdio: 'inherit' })
  children.push(child)
  return child
}

/** 回收所有子进程并退出（防重入：kill 触发的 exit 不再进入 shutdown） */
function shutdown(code) {
  if (shuttingDown) return
  shuttingDown = true
  const exitCode = code ?? 0

  if (IS_WIN) {
    // Windows：child.kill() 只杀直接子进程，且 process.exit 可能先于信号送达；
    // 插件 watch 的孙进程（vite）会孤儿化残留。taskkill /T /F 杀整个进程树
    for (const c of children) {
      if (c.pid) {
        try {
          execFileSync('taskkill', ['/PID', String(c.pid), '/T', '/F'], { stdio: 'ignore' })
        } catch {
          // 已退出，忽略
        }
      }
    }
    process.exit(exitCode)
  }

  // POSIX：kill 后等待所有子进程 exit 再退出（避免信号未送达）。
  // 已退出的子进程不再等其 exit 事件（否则计数永远差一，拖到 2s 超时）。
  // 注意：build.js 的孙进程（vite）不会收到 SIGTERM（Node 不转发信号），
  // 交互式 Ctrl+C 由终端向整个前台进程组广播可覆盖；宿主崩溃/--host-cmd 等
  // 非交互终止会残留 vite（dev 工具可接受，与 Windows taskkill /T 的彻底
  // 回收有差距，此处如实记录）
  let remaining = children.filter((c) => c.exitCode === null).length
  if (remaining === 0) {
    process.exit(exitCode)
    return
  }
  for (const c of children) {
    if (c.exitCode !== null) continue
    c.on('exit', () => {
      remaining -= 1
      if (remaining === 0) process.exit(exitCode)
    })
    try {
      c.kill()
    } catch {
      remaining -= 1
      if (remaining === 0) process.exit(exitCode)
    }
  }
  // 兜底超时：2 秒后强制退出
  setTimeout(() => process.exit(exitCode), 2000).unref()
}

// ==================== 启动 ====================

// 解析 --host-cmd 覆盖（测试/定制用）
const hostIdx = process.argv.indexOf('--host-cmd')
const hostOverride = hostIdx !== -1 && process.argv[hostIdx + 1] ? process.argv[hostIdx + 1] : null
const [hostBin, ...hostArgs] = hostOverride ? hostOverride.split(' ') : DEFAULT_HOST_CMD[1]
const hostCmd = hostOverride ? [hostBin, hostArgs] : DEFAULT_HOST_CMD

// 0. 宿主 devUrl 端口预检（被残留 vite 占用时提前报错，避免“执行不动”假象）
//    + HMR 端口预检（仅 TAURI_DEV_HOST 局域网模式，防“页面正常但热更失效”隐蔽坑）
await precheckDevPort()
await precheckHmrPort()

// 1. 插件前端 watch（先行启动，产物在宿主 resources 同步前就绪）
for (const { dir, args } of PLUGIN_WATCH_CMDS) {
  const child = start(process.execPath, args, resolve(ROOT, dir))
  // 插件 watch 异常退出 → 整组回收（fail fast，避免宿主运行在过期产物上）
  child.on('exit', (code) => {
    if (!shuttingDown) {
      console.error(`[dev-run] 插件 watch 退出（${dir}, code=${code}），回收全部进程`)
      shutdown(code ?? 0)
    }
  })
}

// 2. 宿主 dev 进程
const host = start(hostCmd[0], hostCmd[1], ROOT)
host.on('exit', (code) => {
  if (!shuttingDown) {
    console.log(`[dev-run] 宿主 dev 退出（code=${code}），回收插件 watch`)
    shutdown(code ?? 0)
  }
})

// Ctrl+C / 终止信号：广播回收
process.on('SIGINT', () => shutdown(0))
process.on('SIGTERM', () => shutdown(0))
