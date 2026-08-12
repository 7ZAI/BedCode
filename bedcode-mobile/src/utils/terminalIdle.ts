/**
 * 终端空闲（CLI 回到提示符等待输入）判定
 *
 * 桌面端会话状态事件目前只有 running/stopped 两类会实际变化（waitingInput
 * 检测与插件 taskStatus 均为预留、未接入），移动端无法从协议层得知 agent
 * 是「生成中」还是「停在提示符等输入」。这里用 xterm 缓冲末行文本推断：
 * 末行形似 CLI 提示符 / 提问行，即视为空闲（用户可发送下一条指令）。
 *
 * 注意：本判定只回答「末行是否提示符」，是否真正空闲还需配合「近期无输出」
 * 门控（见 TerminalView 的 outputActivity）——生成中流式输出的代码块引用行
 * （如 `> quote`）也会命中，双条件可把误判偏置到「显示发送而非中断」的安全侧。
 */

/** 空闲提示符/提问行模式（行文本，已剥离 ANSI） */
const IDLE_LINE_PATTERNS: RegExp[] = [
  // 行首提示符：Claude Code/pi 的 `> `、opencode/fish 的 `❯ `（含提示符上已输入的内容）
  /^[>❯] ?/,
  // 带前缀提示符（codex/pi/opencode 自定义 prompt）
  /^(codex|pi|opencode)> ?/,
  // PowerShell 全行提示符（`PS C:\>`）
  /^PS [^>]+> ?$/,
  // 行尾提示符：bash `$ `、root `# `、zsh `% `、Windows cmd `>`、fish `> `
  /^[^>]*[$#>%] ?$/,
  // 确认类提问（CC 权限/继续确认）：发送文本即作答
  /(?:\(|\[)[Yy]\/[Nn](?:\]|\))\s*$/,
  // 行尾提问（agent 等待用户回答）
  /\?\s*$/,
  /^press any key/i,
]

/** 判断一行终端文本是否形似空闲提示符/提问行 */
export function isIdlePromptLine(line: string): boolean {
  return IDLE_LINE_PATTERNS.some(p => p.test(line))
}
