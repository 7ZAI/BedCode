#!/usr/bin/env bash
# ==================== Skill 共享桥接同步脚本 ====================
# 统一 skills 目录：.agents/skills/（唯一真源，git 跟踪）
#   - pi / OpenCode / Codex 原生读取项目级 .agents/skills/，无需桥接
#   - Claude Code 只读 .claude/skills/，本脚本为每个 skill 建立链接：
#       Windows -> 目录 junction（mklink /J，无需管理员权限）
#       Unix    -> symlink（ln -s）
# 幂等：目标已指向正确源则跳过；指向错误则重建；实体目录则提示人工处理（不自动删除）。
# 用法：sh scripts/sync-skills.sh（clone 后每台机器执行一次即可）

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_ROOT/.agents/skills"
DEST_ROOT="$REPO_ROOT/.claude/skills"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) IS_WINDOWS=1 ;;
  *) IS_WINDOWS=0 ;;
esac

# 路径归一化（统一正斜杠、小写盘符），用于跨 Windows/Unix 比较链接指向
normalize() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -am "$1" | tr '[:upper:]' '[:lower:]'
  else
    printf '%s' "$1"
  fi
}

make_link() {
  local dest="$1" target="$2"
  mkdir -p "$(dirname "$dest")"
  if [ "$IS_WINDOWS" = "1" ]; then
    # git-bash 的 MSYS 参数转换会破坏 cmd /c 内嵌引号，改用临时 .bat 文件执行
    # -a 强制绝对路径（mklink 在 cmd 的 cwd 解析相对路径会失败）
    local bat rc
    bat="$(mktemp -d)/mk-link.bat"
    printf '@echo off\r\nmklink /J "%s" "%s"\r\n' "$(cygpath -wa "$dest")" "$(cygpath -wa "$target")" > "$bat"
    cmd //c "$(cygpath -wa "$bat")" >/dev/null 2>&1
    rc=$?
    rm -rf "$(dirname "$bat")"
    return $rc
  else
    ln -s "$target" "$dest"
  fi
}

ensure_link() {
  local name="$1"
  local dest="$DEST_ROOT/$name"
  local target="$SRC/$name"

  if [ -e "$dest" ] || [ -L "$dest" ]; then
    if [ -L "$dest" ]; then
      local cur cur_norm target_norm
      cur="$(readlink "$dest" 2>/dev/null || true)"
      cur_norm="$(normalize "$cur")"
      target_norm="$(normalize "$target")"
      if [ -n "$cur_norm" ] && [ "$cur_norm" = "$target_norm" ]; then
        printf 'ok     %s\n' "$name"
        return
      fi
      rm "$dest"
      printf 'relink %s  (%s -> %s)\n' "$name" "$cur" "$target"
    else
      printf 'skip   %s: 已存在实体目录，需人工处理（先确认内容再手动迁移）\n' "$name" >&2
      return
    fi
  fi

  if make_link "$dest" "$target"; then
    printf 'link   %s\n' "$name"
  else
    printf 'error  %s: 创建链接失败（junction 需目标盘为 NTFS）\n' "$name" >&2
  fi
}

main() {
  [ -d "$SRC" ] || { echo "缺少统一 skills 目录: $SRC" >&2; exit 1; }
  mkdir -p "$DEST_ROOT"

  local count=0
  for d in "$SRC"/*/; do
    [ -d "$d" ] || continue
    [ -f "$d/SKILL.md" ] || continue
    ensure_link "$(basename "$d")"
    count=$((count + 1))
  done

  echo "完成：$count 个 skill 已同步到 $DEST_ROOT"
}

main
