#!/usr/bin/env node
/**
 * Dev Runner（移动端）— 并行启动插件前端 watch 构建与宿主 dev 命令
 *
 * `npm run tauri:android:dev` 一条命令即完成：
 *   - 各插件前端 watch：bedcode-plugin build --watch，改源码自动重建 + 复制产物
 *     （复制到 src-tauri/resources/plugins/mobile/<id>/，Android 侧需手动重新激活插件生效）
 *   - 宿主 dev 进程（tauri android dev）
 * 任一子进程退出（Ctrl+C / 宿主崩溃）时统一回收全部。
 *
 * 用法：
 *   node scripts/dev-run.js              # 默认：三个插件 watch + tauri android dev
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

// ==================== 预检 ====================

/** 从 tauri.conf.json 读取 devUrl 端口（0 = 未配置/解析失败，预检跳过） */
function readDevPort() {
  try {
    const conf = JSON.parse(readFileSync(resolve(ROOT, 'src-tauri/tauri.conf.json'), 'utf-8'))
    const port = Number(new URL(conf?.build?.devUrl).port)
    return Number.isFinite(port) ? port : 0
  } catch {
    return 0
  }
}

/**
 * 宿主 beforeDevCommand（vite dev server）端口预检。
 *
 * tauri dev 启动时自动运行 beforeDevCommand（npm run dev → vite），若端口被残留进程
 * 占用（常见于上次 dev 会话 Ctrl+C 后 vite 未退出），宿主启动必失败且表现像卡死
 * （无提示等待 → “Port is already in use” 报错）。此处提前检测并给出明确指引。
 */
async function precheckDevPort() {
  const port = readDevPort()
  if (!port) return
  const devUrl = `http://localhost:${port}`

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
 * adb reverse 隧道预检（Android WebView 热更新依赖）。
 *
 * WebView 加载 devUrl（http://localhost:<port>）时，设备上的 localhost 指向设备自身，
 * 必须靠 adb reverse 把设备端口转发到宿主机 vite。Tauri CLI 仅在 dev 会话启动时设置
 * 一次，设备拔插重连 / adb kill-server 后隧道丢失且不会自动重建 → 前端改动不热更、
 * 甚至页面静默加载失败（devUrl 不可达命中缓存）。此处幂等重建并提示。
 * 失败不阻断：设备未连接时 android dev 本身会失败，由宿主报错即可。
 */
async function precheckAdbReverse() {
  const port = readDevPort()
  if (!port) return
  try {
    execFileSync('adb', ['reverse', `tcp:${port}`, `tcp:${port}`], { stdio: 'ignore' })
    console.log(`[dev-run] adb reverse tcp:${port} 已建立（Android WebView 热更新通道）`)
  } catch {
    console.warn(`[dev-run] ⚠ adb reverse tcp:${port} 失败（adb 不可用或设备未连接）`)
    console.warn('[dev-run]   设备连接后热更新依赖该隧道；如遇“改代码不刷新”请手动执行：')
    console.warn(`[dev-run]   adb reverse tcp:${port} tcp:${port}`)
  }
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
  {
    dir: 'plugins/ai-chatbox',
    args: [
      resolve(ROOT, 'packages/plugin-sdk-mobile/bin/cli.js'),
      'build', '--watch',
      '--resources-dir', '../../src-tauri/resources/plugins/mobile',
    ],
  },
  {
    dir: 'plugins/auto-task',
    args: [
      resolve(ROOT, 'packages/plugin-sdk-mobile/bin/cli.js'),
      'build', '--watch',
      '--resources-dir', '../../src-tauri/resources/plugins/mobile',
    ],
  },
  {
    dir: 'plugins/file-transfer',
    args: [
      resolve(ROOT, 'packages/plugin-sdk-mobile/bin/cli.js'),
      'build', '--watch',
      '--resources-dir', '../../src-tauri/resources/plugins/mobile',
    ],
  },
]

/** 宿主 dev 命令（可用 --host-cmd 覆盖） */
const DEFAULT_HOST_CMD = [process.execPath, [NPM_CLI, 'run', 'tauri', '--', 'android', 'dev']]

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

  // POSIX：kill 后等待所有子进程 exit 再退出（避免信号未送达）
  let remaining = children.length
  if (remaining === 0) {
    process.exit(exitCode)
    return
  }
  for (const c of children) {
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

// 0. 预检：宿主 devUrl 端口占用（被残留 vite 占用时提前报错，避免“执行不动”假象）
//    + adb reverse 隧道重建（设备拔插后热更新通道会丢，Tauri CLI 不自动恢复）
await precheckDevPort()
await precheckAdbReverse()

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
