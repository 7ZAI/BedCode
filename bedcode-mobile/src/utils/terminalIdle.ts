/**
 * 终端空闲（CLI 回到提示符等待输入）判定
 *
 * 当前无消费者（P0-1「生成中一键中断」已取消：无法准确感知会话输出何时真正
 * 结束，见 .scratch/mobile-input-experience/plan.md）。保留本工具，待桌面端
 * waitingInput 检测或插件 taskStatus 真正接入后，作为忙闲判定的补充信号复用。
 *
 * 判定逻辑：末行形似 CLI 提示符 / 提问行，即视为空闲（用户可发送下一条指令）。
 * 注意：本判定只回答「末行是否提示符」，是否真正空闲还需配合「近期无输出」
 * 门控——生成中流式输出的代码块引用行（如 `> quote`）也会命中。
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
