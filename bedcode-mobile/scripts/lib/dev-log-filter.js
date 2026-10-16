/**
 * dev-log 噪音判定（供 android-dev-log.js 使用）
 *
 * 判定分两层，顺序固定：
 *   ① 行内容规则（DROP_PATTERNS）：设备侧库内部输出（wasmtime/cranelift、mdns_sd、
 *      dev 服务代理连接、MIUI 属性探测告警）与主机侧 tauri CLI / Gradle（任务、
 *      transform、缓存原因）/ cargo-mobile2 / Vite 构建进展
 *   ② logcat tag 白名单：设备侧非本方框架 tag 一律丢弃；非 logcat 行（主机侧）不套用
 *
 * —— 为什么过滤 dev 服务代理连接日志 ——
 * 移动端 dev（`tauri android dev`）下 WebView 的地址是 `tauri.localhost`：Tauri 的
 * `tauri://` protocol handler 在 dev+mobile 构建里用 reqwest 把**每个 WebView 请求**
 * 转发到 `src-tauri/tauri.conf.json` 的 `build.devUrl`（vite dev server，经 `adb reverse`
 * 落在设备 127.0.0.1:<port>）。模块加载 / HMR / 插件前端 import 每请求都会让
 * reqwest / hyper_util 各打一条 debug 连接日志 —— 实测 3.5 分钟 1407 行、占该次
 * dev 日志 ~7 成（全部指向 127.0.0.1:1423），是纯粹的框架内部噪音。
 *
 * 过滤范围**只限 devUrl 指向的 host:port**，链路排障日志照常保留：
 *   - 带端口的 connect / pool 行按 devUrl 的 host:port 精确匹配（局域网对端连的是
 *     其他 host/端口，不受影响）；
 *   - 不带端口的 `reqwest::connect: starting new connection` 行只在 devUrl host 是
 *     回环地址时过滤：真机上 127.0.0.1 只可能是 adb reverse 的 dev 代理（局域网对端
 *     是 LAN IP），不会误杀；devUrl 指向 LAN IP（`--host` 模式）时该行不套用 ——
 *     同一 IP 上可能是真实对端连接，宁可保留。
 */
const LOGCAT_LINE = /^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d+\s+\d+\s+\d+\s+[VDIWEF]\s+/

/**
 * logcat 行补年份前缀（落盘用）：设备 logcat 时间戳是 `MM-DD HH:MM:SS.mmm`，
 * 无年份；落盘文件按本地日期命名，跨年/归档后时间线断裂。本函数给 logcat 行
 * 首加 `YYYY-`（年份 = 脚本本地年，与文件日期线一致）；非 logcat 行原样返回
 * （主机侧构建噪音等无此格式，不应加）。仅用于落盘路径，控制台保持原始形态。
 */
export function withYearPrefix(line, year = new Date().getFullYear()) {
  return LOGCAT_LINE.test(line) ? `${year}-${line}` : line
}

/** 从 logcat 行提取 tag；非 logcat 行返回 null（主机侧输出无 tag） */
function logcatTag(line) {
  const m = LOGCAT_LINE.exec(line)
  if (!m) return null
  const rest = line.slice(m[0].length)
  const colon = rest.indexOf(':')
  if (colon <= 0) return null
  return rest.slice(0, colon).trim()
}

/**
 * 本方标签族：BedCode 前缀覆盖 Rust tracing 主 tag 与 Kotlin 侧 BedCode-* 系列；
 * 另保留各崩溃关键框架 tag（Java crash FATAL / 原生 Fatal signal / tombstone）。
 * 新设备出现的新框架 tag 无需维护——不在白名单即自动滤除。
 */
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

/** 缩继续行（栈帧 \tat / gradle 详情等）：跟随上一行的丢弃结论 */
export const isContinuation = (line) => /^\s+\S/.test(line)

// —— 行内容模式判定丢弃（覆盖 logcat 的 BedCode tag 内部与主机侧输出）——
const DROP_PATTERNS = [
  // wasmtime/cranelift JIT 编译内部（插件加载时洪水刷屏，实测单次 dev 日志占 ~9 成）
  /cranelift_codegen::/,
  /wasmtime_internal_cranelift::/,
  /wasmtime_cranelift::/,
  /wasmtime::runtime::code_memory/,
  // mdns_sd crate 内部 debug（业务发现日志在 bedcode_mobile_lib::mdns::* / bedcode_peer_net::*）
  /mdns_sd::/,
  // MIUI/设备属性探测告警（libc tag 保留给崩溃信号行，这类 property 探测恒被拒、无排障价值）
  /Access denied finding property/,
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
  /Caching disabled for task ':/, // Gradle 8.x 行首无缩进（旧的缩进版正则漏放 593 行/次）
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
  /^Skipping DexingNoClasspathTransform:/, // 实际输出带 "Skipping " 前缀，上一条锚定匹配不到
  /^> Transform /, // Gradle transform 进展（"bundleLibRuntimeToDirDebug ..."）
  /^ *Info \[cargo_mobile2::android::/, // cargo-mobile2 的 so 链接/libs 拷贝进展（非警告/错误）
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
  /^dist\/\S+\s+[\d.,]+ kB │ gzip: /, // Vite 每产物体积汇总行
  /^vite v\d+\.\d+\.\d+ building client environment/,
  // 插件 watch 的重复 banner（「产物已复制 / 自动复制到」行保留）
  /^\[bedcode-plugin\] =+/,
  /^\[bedcode-plugin\] Ctrl\+C 退出/,
  /^\[bedcode-plugin\] 修改插件前端源码后自动重建/,
]

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * devUrl → `{ host, port }`；缺失 / 非法 / 无显式端口返回 null（调用方按「该层过滤未启用」处理）
 *
 * devUrl 真源 = `src-tauri/tauri.conf.json` 的 `build.devUrl`（vite dev server）。
 */
export function parseDevProxyTarget(devUrl) {
  if (!devUrl) return null
  let url
  try {
    url = new URL(devUrl)
  } catch {
    return null
  }
  if (!url.port) return null
  // URL.hostname 对 IPv6 带方括号（[::1]），日志里也只以该形态出现，去掉后统一由
  // 调用方按需拼回带括号形式（见 createDevProxyPatterns）
  return { host: url.hostname.replace(/^\[|\]$/g, ''), port: url.port }
}

const LOOPBACK_HOSTS = ['127.0.0.1', 'localhost', '[::1]']

/**
 * dev 服务代理连接日志的丢弃正则；devUrl 不可用返回空数组（不启用该层过滤）
 *
 * 只匹配 devUrl 的 host:port，且 `host` 为 URL 字面值：devUrl 写 `localhost` 时设备
 * 实际连的是 `127.0.0.1`（reqwest 解析后），故回环场景把几个等价写法一并纳入。
 */
function createDevProxyPatterns(devUrl) {
  const target = parseDevProxyTarget(devUrl)
  if (!target) return []
  const isLoopback = ['127.0.0.1', 'localhost', '::1'].includes(target.host)
  const hosts = (isLoopback ? LOOPBACK_HOSTS : [target.host]).map(escapeRegExp).join('|')
  const hostPort = `(?:${hosts}):${escapeRegExp(target.port)}(?!\\d)`
  const patterns = [
    new RegExp(`hyper_util::client::legacy::connect::http: (?:connecting|connected) to ${hostPort}`),
    new RegExp(`hyper_util::client::legacy::pool: pooling idle connection for \\("http", ${hostPort}\\)`),
  ]
  if (isLoopback) {
    patterns.push(new RegExp(`reqwest::connect: starting new connection 'Some\\("(?:${hosts})"\\)'`))
  }
  return patterns
}

/**
 * 创建有状态噪音判定器（缩继续行依赖「上一行是否被丢弃」，故状态封装在闭包内）
 *
 * @param {{ devUrl?: string | null, noFilter?: boolean }} [options]
 *   devUrl   —— devUrl，用于生成 dev 服务代理连接过滤；缺失即不启用该层
 *   noFilter —— true 时完全关闭过滤（BEDCODE_LOG_NO_FILTER=1 全量输出）
 */
export function createNoiseFilter({ devUrl = null, noFilter = false } = {}) {
  const patterns = [...DROP_PATTERNS, ...createDevProxyPatterns(devUrl)]
  let prevLineDropped = false

  return {
    /** 判定一个完整行（入参必须是剥色后的文本：ANSI 会破坏行首锚定与内容正则） */
    isNoise(line) {
      let noise
      if (noFilter) {
        noise = false
      } else if (patterns.some((re) => re.test(line))) {
        noise = true
      } else {
        const tag = logcatTag(line)
        noise = tag ? !isKeepLogcatTag(tag) : prevLineDropped && isContinuation(line)
      }
      prevLineDropped = noise
      return noise
    },
  }
}
