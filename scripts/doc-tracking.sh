#!/bin/sh
# 分支级文档跟踪助手（doc-tracking）
#
# 背景：文档/配置文件（docs/、AGENTS.md、CLAUDE.md、CONTEXT.md、.pi 配置）只在跟踪分支
# （默认 dev）入库；其他分支（master / uat / milestone 等）不跟踪。
# .gitignore 已忽略这些路径，因此非跟踪分支剔除后它们在工作区中保持"被忽略"
# 状态，来回切换分支不会冲突。
#
# 用法：
#   scripts/doc-tracking.sh untrack [hook]  非跟踪分支从 index 剔除受保护文件
#                                           （工作区保留；同时以"删除"侧解决
#                                           dev→master 合并产生的 modify/delete 冲突）
#   scripts/doc-tracking.sh restore         非跟踪分支上从跟踪分支恢复缺失的
#                                           工作区文件（供本地查阅，不入库）
#
# 由 scripts/hooks/ 下的 pre-commit / post-checkout / post-merge 自动调用，
# 也可手动运行（如合并冲突后运行 untrack 再提交）。
#
# 环境变量：DOC_TRACKING_BRANCHES 可覆盖跟踪分支白名单（默认 "dev"）。

TRACKING_BRANCHES="${DOC_TRACKING_BRANCHES:-dev}"

# 受保护路径，与 .gitignore 的 Documentation / IDE 段落对应。
# 注意：.pi 只跟踪配置（agents/extensions/prompts/settings.json），
# .pi/sessions/ 会话日志始终忽略、不入库（勿执行 git add -f .pi 整目录）。
PROTECTED_PATHS="docs AGENTS.md CLAUDE.md CONTEXT.md .pi"

# ==================== 工具函数 ====================

current_branch() {
  git symbolic-ref --short HEAD 2>/dev/null
}

is_tracking_branch() {
  for _b in $TRACKING_BRANCHES; do
    [ "$1" = "$_b" ] && return 0
  done
  return 1
}

# ==================== 子命令 ====================

# 从 index 剔除受保护文件（保留工作区内容）。
# git ls-files 会列出冲突条目，因此 git rm --cached 同时能把
# modify/delete 冲突解决为"保持删除"。
cmd_untrack() {
  _hook="${1:-manual}"
  _branch=$(current_branch)
  # detached HEAD 等无分支场景不处理
  [ -z "$_branch" ] && return 0
  is_tracking_branch "$_branch" && return 0

  _removed=1
  for _p in $PROTECTED_PATHS; do
    if [ -n "$(git ls-files -- "$_p")" ]; then
      git rm -r --cached --quiet --ignore-unmatch -- "$_p" || return 1
      _removed=0
    fi
  done

  if [ "$_removed" -eq 0 ]; then
    echo "[doc-tracking] 分支 '$_branch' 不跟踪文档文件，已从 index 剔除（工作区保留）。"
    case "$_hook" in
      post-merge | post-checkout | manual)
        echo "[doc-tracking] 剔除以暂存删除形式存在，请随下次提交落库（或 git commit -m 'chore: untrack docs'）。"
        ;;
    esac
  fi
  return 0
}

# 从跟踪分支恢复工作区中缺失的受保护文件（仅工作区，不入库）。
# 场景：从 dev 切到 master 时，dev 跟踪而 master 不跟踪的文件会被 checkout
# 从工作区删除，此命令把它们恢复出来供本地查阅。
cmd_restore() {
  _branch=$(current_branch)
  [ -z "$_branch" ] && return 0
  is_tracking_branch "$_branch" && return 0

  for _p in $PROTECTED_PATHS; do
    for _tb in $TRACKING_BRANCHES; do
      git rev-parse --verify --quiet "$_tb" >/dev/null 2>&1 || continue
      git ls-tree -r --name-only "$_tb" -- "$_p" 2>/dev/null |
        while IFS= read -r _f; do
          [ -e "$_f" ] || git restore --source="$_tb" --worktree -- "$_f" 2>/dev/null
        done
    done
  done
  return 0
}

# ==================== 入口 ====================

case "${1:-}" in
  untrack)
    shift
    cmd_untrack "$@"
    ;;
  restore)
    cmd_restore
    ;;
  *)
    echo "用法: $0 {untrack [hook]|restore}" >&2
    exit 2
    ;;
esac
