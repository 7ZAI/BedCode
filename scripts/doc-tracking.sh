#!/bin/sh
# 分支级文档跟踪助手（doc-tracking）
#
# 背景：文档/配置文件（docs/、CLAUDE.md、CONTEXT.md、.pi 配置、.scratch issue 文档）
# 只在除 uat / master 外的分支入库（dev、feature/* 等全部正常跟踪）。
# uat / master 不跟踪这些路径——仅从 index 剔除，工作区始终保留（.gitignore 已忽略
# 这些路径，且 post-checkout 会从 dev 恢复工作区副本），来回切换分支不会冲突，
# 也不会产生"删除文档"的提交。
#
# 注意：README.md / README_en.md 与 AGENTS.md 不在受保护路径中，所有分支（含 uat/master）均正常跟踪。
#
# 用法：
#   scripts/doc-tracking.sh untrack [hook]  uat/master 从 index 剔除受保护文件
#                                           （工作区保留；同时以"删除"侧解决
#                                           dev→uat/master 合并产生的 modify/delete 冲突）
#   scripts/doc-tracking.sh restore         uat/master 上从 dev 恢复缺失的工作区文件
#                                           （供本地查阅，不入库）
#
# 由 scripts/hooks/ 下的 pre-commit / post-checkout / post-merge 自动调用，
# 也可手动运行（如合并冲突后运行 untrack 再提交）。
#
# 环境变量：
#   DOC_UNTRACKED_BRANCHES   不跟踪分支黑名单（默认 "uat master"），其余分支全部正常跟踪
#   DOC_TRACKING_SOURCE      恢复工作区副本的源分支（默认 "dev"）

UNTRACKED_BRANCHES="${DOC_UNTRACKED_BRANCHES:-uat master}"
TRACKING_SOURCE="${DOC_TRACKING_SOURCE:-dev}"

# 受保护路径，与 .gitignore 的 Documentation / IDE 段落对应。
# README.md / README_en.md 与 AGENTS.md 不在此列（全分支跟踪）。
# 注意：.pi 只跟踪配置（agents/extensions/prompts/settings.json），
# .pi/sessions/ 会话日志始终忽略、不入库（勿执行 git add -f .pi 整目录）。
PROTECTED_PATHS="docs CLAUDE.md CONTEXT.md .pi .scratch"

# ==================== 工具函数 ====================

current_branch() {
  git symbolic-ref --short HEAD 2>/dev/null
}

# 当前分支是否在黑名单（不跟踪文档）中
is_untracked_branch() {
  for _b in $UNTRACKED_BRANCHES; do
    [ "$1" = "$_b" ] && return 0
  done
  return 1
}

# ==================== 子命令 ====================

# 从 index 剔除受保护文件（保留工作区内容）。仅在 uat / master 上执行。
# dev、feature 等其余分支直接返回，正常跟踪、不干预。
cmd_untrack() {
  _hook="${1:-manual}"
  _branch=$(current_branch)
  # detached HEAD 等无分支场景不处理
  [ -z "$_branch" ] && return 0
  # 非黑名单分支（dev、feature/* 等）：正常跟踪，不干预
  is_untracked_branch "$_branch" || return 0

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

# 从跟踪源分支（默认 dev）恢复工作区中缺失的受保护文件（仅工作区，不入库）。
# 场景：uat/master 上 dev 合入的新文档在工作区缺失时补回，供本地查阅。
cmd_restore() {
  _branch=$(current_branch)
  [ -z "$_branch" ] && return 0
  # 非黑名单分支不需要恢复（本就跟踪）
  is_untracked_branch "$_branch" || return 0

  git rev-parse --verify --quiet "$TRACKING_SOURCE" >/dev/null 2>&1 || return 0
  for _p in $PROTECTED_PATHS; do
    git ls-tree -r --name-only "$TRACKING_SOURCE" -- "$_p" 2>/dev/null |
      while IFS= read -r _f; do
        [ -e "$_f" ] || git restore --source="$TRACKING_SOURCE" --worktree -- "$_f" 2>/dev/null
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
