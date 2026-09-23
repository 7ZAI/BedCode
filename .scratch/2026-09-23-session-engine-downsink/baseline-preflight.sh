#!/usr/bin/env bash
# ==============================================================================
# 票 01「桌面功能等价人工基线」的起跑前置检查
#
# 用途：基线要求「以当前 dev 为准」，但本仓库常有并发批次在同一 worktree 上写盘，
# 且共享 src-tauri/target（无法另开 worktree 跑干净树）。所以每次复跑都必须先回答
# 三个问题，否则清单里发现的差异无法判归属：
#   1. 跑的是哪个树（HEAD 提交 + 未提交改动清单 + 每处改动属于哪条线）
#   2. 起跑会不会被挡（ensurePluginWasm 补建预演 + 宿主 lib 是否可编译）
#   3. 跑完去哪儿取证（当日日志路径与关键锚点）
#
# 用法：
#   bash .scratch/2026-09-23-session-engine-downsink/baseline-preflight.sh
#   bash .../baseline-preflight.sh --compile      # 追加 cargo check --lib（慢，首次可达数分钟）
#   bash .../baseline-preflight.sh --save          # 把输出落成 .scratch/<task>/preflight-<日期>.txt
#
# 本脚本只读取状态，不修改任何文件（--save 只写自身报告）。
# ==============================================================================
set -euo pipefail

DO_COMPILE=0
DO_SAVE=0
for a in "$@"; do
  case "$a" in
    --compile) DO_COMPILE=1 ;;
    --save)    DO_SAVE=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "未知参数：$a（支持 --compile|--save|--help）" >&2; exit 2 ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DESKTOP="$REPO_ROOT/bedcode-desktop"
TASK_DIR="$REPO_ROOT/.scratch/2026-09-23-session-engine-downsink"
TODAY="$(date +%F)"
RUN_LOG="$HOME/.local/share/com.bedcode.app/logs/runtime.$TODAY.log"

if [ "$DO_SAVE" = "1" ]; then
  OUT="$TASK_DIR/preflight-$TODAY-$(date +%H%M).txt"
  exec > >(tee "$OUT") 2>&1
fi

cd "$REPO_ROOT"
BLOCKERS=()

echo "=============== 票 01 人工基线 · 起跑前置 ==============="
echo "时间          : $(date '+%F %T %Z')"

# ==================== 1. 基线锚点与工作区污染 ====================
echo
echo "----- [1] 基线锚点 -----"
echo "分支          : $(git branch --show-current)"
echo "HEAD          : $(git log -1 --format='%h %s')"
echo "会话下沉进度  : 票 02/04/05/13/14 已 landed（票 13 门住本票能否起跑）"

echo
echo "----- [2] 未提交改动与归属 -----"
# 归属表：本专项的基线只跑终端窗口面；并发票 06 改的是移动端输出通道与历史快照，
# 二者在 session_gateway / websocket 上有交集，所以「对侧在途」必须显式记账。
owner_of() {
  case "$1" in
    *.scratch/2026-09-23-session-engine-downsink/*)
      echo "本专项 · 会话引擎下沉（票文档/配套，不影响运行态）" ;;
    *server/websocket*|*utils/session_gateway*|*terminal_ws*|*channel/terminal.rs|*pty/*|\
    *tests/pty_session_chain.rs|*tests/ws_session_route.rs|*http/controllers/session_controller.rs)
      echo "并发批次 · 票 06（移动端输出通道/历史直读引擎环）" ;;
    docs/diagrams/*)               echo "文档线 · 架构图重绘" ;;
    *.scratch/2026-09-24-host-crypto*) echo "并发批次 · host-crypto 线（未跟踪）" ;;
    *)                             echo "未归属 ← 需人工确认" ;;
  esac
}
DIRTY_TRACKED=$(git status --porcelain | awk '$1=="M"||$1=="MM"||$1=="AM"{print $2}')
DIRTY_UNTRACKED=$(git status --porcelain | awk '$1=="?"{print $2}')
if [ -z "$DIRTY_TRACKED" ]; then
  echo "工作区干净：基线 = HEAD 态，归属判定最干净"
else
  n=0
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    printf '  %-62s ← %s\n' "$f" "$(owner_of "$f")"
    n=$((n+1))
  done <<< "$DIRTY_TRACKED"
  echo "在途已跟踪文件：$n 个"
  if echo "$DIRTY_TRACKED" | grep -q "server/websocket\|utils/session_gateway"; then
    echo "  ⚠ 含并发票 06 在途改动：本轮基线跑的是「半成品树」，"
    echo "    终端回放/输出面若出差异，必须先判是 06 的 WIP 还是 P1-b 回归。"
    BLOCKERS+=("并发票 06 在途：基线非稳定态")
  fi
fi
[ -n "$DIRTY_UNTRACKED" ] && echo "未跟踪路径：$(echo "$DIRTY_UNTRACKED" | tr '\n' ' ')"

# ==================== 3. 插件产物预演 ====================
# 与 scripts/dev-run.js 的 ensurePluginWasm()/wasmStaleReason() 同形：
# 比的是 resources 里的产物 vs（插件 rust/ ∪ SDK rust/）最新 mtime，
# 且只有 PLUGIN_WATCH_CMDS 列出的三条参与补建门禁。
echo
echo "----- [3] 插件产物预演（dev 起跑时是否补建）-----"
STALE_LIST=$(cd "$DESKTOP" && node -e '
const {readdirSync,statSync}=require("fs");const {join,resolve,basename}=require("path");
const ROOT=process.cwd();
function latest(root){let m=0;const skip=new Set(["target","dist","node_modules"]);const st=[root];
 while(st.length){const d=st.pop();let es;try{es=readdirSync(d,{withFileTypes:true})}catch{continue}
  for(const e of es){const p=join(d,e.name);
   if(e.isDirectory()){if(!skip.has(e.name))st.push(p)} else {try{const t=statSync(p).mtimeMs;if(t>m)m=t}catch{}}}}
 return m}
const cmds=[["plugins/ai-chatbox","com.bedcode.ai-chatbox","bedcode_plugin_ai_chatbox.wasm"],
 ["plugins/terminal-session","com.bedcode.terminal-session","bedcode_plugin_terminal_session.wasm"],
 ["plugins/file-transfer","com.bedcode.file-transfer","bedcode_plugin_file_transfer.wasm"]];
const RES=resolve(ROOT,"src-tauri/resources/plugins/desktop");
const sdk=join(ROOT,"packages/plugin-sdk-desktop/rust");
const out=[];
for(const [dir,id,wf] of cmds){
 const dest=resolve(RES,id,wf); let verdict;
 try{const a=statSync(dest);const s=Math.max(latest(join(ROOT,dir,"rust")),latest(sdk));
   if(s>a.mtimeMs){verdict="需补建";out.push(id+" 需补建（源码比产物新）")}
   else verdict="FRESH"}
 catch{verdict="产物缺失→需补建";out.push(id+" 产物缺失")}
 console.error("  "+id.padEnd(32)+verdict);
}
process.exit(out.length?1:0)' 2>&1)
echo "$STALE_LIST" | grep -v "需补建\|产物缺失" || true
if echo "$STALE_LIST" | grep -q "需补建\|产物缺失"; then
  echo "  → ensurePluginWasm 会在起跑时补建；票 13 已修跨插件防漂移，"
  echo "    补建失败的风险已消除，但起跑会慢几十秒（正常，不是回归）"
else
  echo "  → 三条全 FRESH：起跑不触发补建（票 13 的前置满足）"
fi

# ==================== 4. 宿主可编译性 ====================
echo
echo "----- [4] 宿主 lib 可编译性 -----"
if [ "$DO_COMPILE" = "1" ]; then
  echo "跑 cargo check --lib（可能几分钟）…"
  if (cd "$DESKTOP/src-tauri" && cargo check --lib 2>&1 | grep -qE "^error"); then
    echo "  ❌ lib 编译红 → 起跑必挡，先看是不是并发批次在途文件"
    (cd "$DESKTOP/src-tauri" && cargo check --lib 2>&1 | grep -E "^error" -A2 | grep -E "^error|-->" | head -8)
    BLOCKERS+=("宿主 lib 编译红")
  else
    echo "  ✅ lib 编译通过（warning 不影响起跑）"
  fi
else
  echo "  跳过（加 --compile 实跑；本轮结论见上一次会话记录）"
fi

# ==================== 5. 取证位置 ====================
echo
echo "----- [5] 运行期取证 -----"
# 日志文件名按 UTC 不按本地日期：本地 09-24 06:2x 起的一轮落在 runtime.09-23.log。
# 按 $(date +%F) 拼名会指向不存在的文件，grep 空文件 = 假绿「无异常」（2026-09-24 实测）。
LATEST_LOG="$(ls -t "$HOME/.local/share/com.bedcode.app/logs/"runtime.*.log 2>/dev/null | head -1 || true)"
echo "本地日期      : $(date +%F)（UTC $(date -u +%F)）—— 落盘日志按 UTC 命名"
echo "当日日志(票面) : $RUN_LOG  $([ -f "$RUN_LOG" ] && echo 存在 || echo '不存在（正常：UTC 名不同）')"
echo "实取日志(mtime) : ${LATEST_LOG:-尚无}"
if [ -n "$LATEST_LOG" ]; then
  echo "  $([ "$(find "$LATEST_LOG" -mmin -30 2>/dev/null | wc -l)" = 1 ] \
        && echo '30 分钟内写过 → 是本轮的日志' \
        || echo '最近 30 分钟没写过 → 上一轮的残留，起跑后需重新取')"
  echo "  当前 $(( $(wc -l < "$LATEST_LOG") )) 行；跑完只看增量："
  echo "    tail -n +$(( $(wc -l < "$LATEST_LOG") + 1 )) \"$LATEST_LOG\" > /tmp/baseline-run.log"
  echo "  ⚠ 每次 dev 启动会重写当日日志（stdout 见 '[logging] dev reset: replaced today's log'）"
  echo "    → 中途重启 = 前半程证据清零，重启前先把增量另存"
else
  echo "  尚未生成（今日还没跑过 dev）；起跑后会自动创建（名字按 UTC）"
fi
echo
echo "⚠ 起跑后第一件事：确认 com.bedcode.terminal-session 是 Activated 而非 Loaded——"
echo "  P1-b 无降级轨，它没激活则本清单 10 项一条都观测不了（看起来像基线挂了）。"
echo "  判据在 dev stdout：'Initialization complete: … N activated' 与"
echo "  '- com.bedcode.terminal-session (state=Activated, …)'。Loaded 就去插件管理界面启用，"
echo "  不要改持久化状态文件绕。"
echo
echo "关键锚点（会话面 fail-visible 红线，AGENTS §8）："
cat <<'ANCHORS'
  session plugin not active        ← 插件未激活仍被问会话事实 = 必判回归
  refused: session plugin not active
  failed via plugin                ← 互调失败（转发层 error）
  session created via plugin       ← 正常创建路径（info，带 config_id/session_id）
  session stop requested via plugin / session removed via plugin
  [plugin:                           ← 插件 WASM 侧日志统一前缀
ANCHORS
echo
echo "grep 一行式（跑完直接贴进票末 Comments；路径用上面「实取日志」那一条）："
printf '  LOG="%s"\n' "${LATEST_LOG:-\$NOT_YET}"
echo '  grep -E "session plugin not active|failed via plugin|\[plugin:" "$LOG" | tail -40'
echo "  （落盘时间戳是 UTC ISO，按本地时分 grep 恒 0 命中，别拿它当「没发生」）"
echo
echo "目测项无工具可替代：截图工具缺失（无 grim/spectacle/import），"
echo "只有 xdotool + fcitx5 —— IME 组合窗、回显、拽底、「回到底部」按钮时机必须人眼看。"

# ==================== 6. 宿主源编辑抖动（跑人工基线的独占性判据）====================
echo
echo "----- [6] 宿主源近期改动（基线独占性）-----"
# `tauri dev` watch 整个 src-tauri/：并发批次每存一次盘，基线 app 就重启一次、
# 日志被 `[logging] dev reset` 清零一次（2026-09-24 实测十分钟重启 11 次）。
RECENT_SRC=$(find "$DESKTOP/src-tauri/src" -name '*.rs' -newermt '-10 minutes' 2>/dev/null | head -5)
if [ -n "$RECENT_SRC" ]; then
  echo "  ⚠ 最近 10 分钟有宿主源文件被改（很可能有人正在编辑，基线会被反复重启）："
  echo "$RECENT_SRC" | sed 's/^/     /'
  BLOCKERS+=("宿主源 10 分钟内有改动：基线独占性不成立，等其停笔再跑")
else
  echo '  ✅ 最近 10 分钟无宿主源改动 → tauri dev 不会因对侧存盘被反复重启'
fi

# ==================== 7. 结论 ====================
echo
echo "=============== 结论 ==============="
if [ "${#BLOCKERS[@]}" -eq 0 ]; then
  echo "✅ 起跑无阻塞：cd bedcode-desktop && pnpm run tauri:dev"
  echo "   清单见 baseline-run-sheet.md，结果按条记回 issues/01 票末 Comments"
else
  echo "⚠ 起跑前需知晓："
  for b in "${BLOCKERS[@]}"; do echo "   - $b"; done
  echo "   仍可跑（用户 2026-09-24 已裁定「当前工作区照跑 + 票里记账标明污染」），"
  echo "   但每条差异要先过 run-sheet 的归属三问再立票。"
fi
echo "建议同步跑：bash scripts/wasip3-toolchain.sh verify（pinned nightly 就绪与否）"
if [ "$DO_SAVE" = "1" ]; then echo "（本报告已落 $OUT）"; fi
exit 0
