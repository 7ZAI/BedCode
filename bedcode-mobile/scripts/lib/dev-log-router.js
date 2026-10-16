/**
 * dev-log 行缓冲与「控制台 / 落盘」双写通路的纯逻辑（被 android-dev-log.js 使用）
 *
 * 一条日志要同时满足三个互斥要求：
 *   - 控制台：原样透传子进程输出（保留 ANSI 颜色与 \r 进度条覆盖）
 *   - 落盘文件：纯文本（grep / cat 友好，无 ANSI、无 \r）
 *   - 过滤判定：必须跑在剥色后的文本上（ANSI 插在行中间会让正则匹配失准）
 * 因此每个完整行产出两份形态，调用方分别取用：
 *   raw   —— 原文，控制台用
 *   clean —— 去 ANSI 且去 \r 的纯文本，过滤判定 + 落盘用
 *
 * 两条缓冲同步按 \n 切分，行对一一对应；前提是 stripAnsi 不增删换行
 * （OSC 序列显式排除 \n，见下）。
 */

/**
 * 去掉 ANSI 转义序列（落盘与过滤判定用；控制台不走本函数）
 * - CSI 序列：颜色 \x1b[38;5;123m、清屏 \x1b[2J、光标 \x1b[1A、行擦除 \x1b[2K、光标显隐 \x1b[?25l 等
 * - OSC 序列：如终端标题 \x1b]0;...\x07。字符类排除 \x0a：OSC 若跨行吞掉换行，
 *   raw / clean 的行数就会错位（一条 clean 行配不上 raw 行），双写直接串行
 * - \r（进度条覆盖用的回车：纯文本里覆盖不生效，只会把多次进度拼成长行）
 */
export const stripAnsi = (text) =>
  text
    .replace(/\x1b\[[0-9;?]*[ -\/]*[@-~]/g, '')
    .replace(/\x1b\][^\x07\x0a]*(?:\x07|\x1b\\)/g, '')
    .replace(/\r/g, '')

/**
 * 创建行路由器：把解码后的文本 chunk 切成「完整行对」
 *
 * @param {string[]} fds 需要跟踪的输出通道（如 ['stdout', 'stderr']）
 */
export function createLineRouter(fds) {
  const buffers = Object.fromEntries(fds.map((fd) => [fd, { raw: '', clean: '' }]))
  const bufOf = (fd) => {
    const buf = buffers[fd]
    // 未知 fd 是脚本侧接线错误：静默忽略会整条丢日志，必须立刻暴露
    if (!buf) throw new Error(`dev-log 行路由收到未知 fd: ${fd}`)
    return buf
  }

  return {
    /**
     * 追加一段文本，返回本批已完整的行对（末尾半行留到下一批或 flush 才判定）
     * @param {string} fd
     * @param {string} chunk 解码后的文本（可含 ANSI 与 \r）
     * @returns {{ raw: string, clean: string }[]}
     */
    push(fd, chunk) {
      const buf = bufOf(fd)
      buf.raw += chunk
      buf.clean += stripAnsi(chunk)
      const cleanLines = buf.clean.split('\n')
      const rawLines = buf.raw.split('\n')
      buf.clean = cleanLines.pop() ?? ''
      buf.raw = rawLines.pop() ?? ''
      return cleanLines.map((clean, i) => ({ raw: rawLines[i] ?? '', clean }))
    },

    /**
     * 进程退出时取走残留半行（无残留返回 null；同一行不会被取走两次）
     * @param {string} fd
     * @returns {{ raw: string, clean: string } | null}
     */
    flush(fd) {
      const buf = bufOf(fd)
      if (buf.raw === '' && buf.clean === '') return null
      const row = { raw: buf.raw, clean: buf.clean }
      buf.raw = ''
      buf.clean = ''
      return row
    },
  }
}