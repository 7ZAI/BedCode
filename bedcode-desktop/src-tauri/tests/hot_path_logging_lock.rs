//! 热路径日志风暴 · 防回接锁
//!
//! 对应 AGENTS.md §8 日志红线②「热路径克制（PTY 输出 / WS 每帧不打逐帧日志，
//! 高频 API 走 `debug!`）」。这条红线里真正会出事的一半不是「级别选错」而是
//! **在按消息 / 按帧 / 按文件操作触发的位置写了日志**：级别再低，单条日志也会
//! 按触发频率线性膨胀，把开发日志冲成噪声、淹没真正的 warn / error。
//!
//! ## 为什么要测试而不是文档
//!
//! `tracing::debug!` 加一行既不编译失败、也不测试变红，只在运行期静悄悄地把日志
//! 冲掉。三处都在热路径上，删掉日志没有任何功能影响——正因如此只能靠锁。
//!
//! ## 实测事故（2026-10-03 桌面端 dev-run 日志）
//!
//! | 站点 | 触发频率 | 一次会话实测 | 占该日志字节 |
//! |---|---|---|---|
//! | `MessageBus::dispatch_publish` 投递留痕 | PTY 输出通知 ≥50ms/句柄（≈16/s） | 9 585 行 / 11 分钟 | 51.7% |
//! | `fs_auth::check` 免弹窗放行 | 每次文件读写（一次全盘用量扫描 = 一个文件一行） | 单分钟 1 121 行 | 12.2% |
//! | `record_dropped_frame` 非首次丢帧 | 每帧 | 当日 0 行（休眠路径，但形状相同） | — |
//!
//! 三者内容全是零信息重复（`enqueued=1/1 rejected=0`、`allowed`、`dropped`），
//! 逐条留痕没有任何诊断价值。
//!
//! ## 锁的口径与边界
//!
//! 锁的是**这几个函数体内不得出现 `tracing::debug!`**，不是「全仓禁用 debug」。
//! 按会话 / 按连接 / 按激活 / 按生命周期事件的 debug 完全合法且必要——那些是
//! 低频事件，每次一条正是排查所需的对照信息。全仓一刀切会把可观测性一起砍掉，
//! 那比日志噪声更糟。
//!
//! 异常分支的 `warn!` / `error!` **不受本锁约束**：队列满、格式不匹配、用户拒绝、
//! 规范化失败这些是真正的异常，留痕是义务（`MessageBus::dispatch_publish` 的
//! 队列满 / 格式不匹配、`fs_auth` 的拒绝与弹窗路径都仍逐条 warn）。

use std::fs;
use std::path::PathBuf;

// ==================== 源码扫描原语（纯函数，可被 fixture 测试） ====================

/// 把源码里的注释与字符串字面量抹成等长空白，保留换行。
///
/// 目的只有一个：让后续的花括号配对不被注释和字符串里的 `{` / `}` 带偏
/// （例如日志消息里的 `'{}'` 占位符）。**不改变任何字符偏移**——偏移保持不变是
/// 本函数存在的全部意义，扫描结果要能直接对回原文行号报给人看。
fn blank_out_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    // 行注释与块注释各自独立的「未闭合」状态：块注释可跨行，行注释到行尾即止
    let mut in_block_comment = false;
    let mut in_line_comment = false;
    let mut in_string = false;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();

        if in_line_comment {
            if b == b'\n' {
                in_line_comment = false;
                out.push('\n');
            } else {
                out.push(' ');
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if b == b'*' && next == Some(b'/') {
                out.push(' ');
                out.push(' ');
                i += 2;
                in_block_comment = false;
                continue;
            }
            out.push(if b == b'\n' { '\n' } else { ' ' });
            i += 1;
            continue;
        }

        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            out.push(if b == b'\n' { '\n' } else { ' ' });
            i += 1;
            continue;
        }

        // 非字符串区：判定注释与字符串的开启
        if b == b'/' && next == Some(b'/') {
            in_line_comment = true;
            out.push(' ');
            out.push(' ');
            i += 2;
            continue;
        }
        if b == b'/' && next == Some(b'*') {
            in_block_comment = true;
            out.push(' ');
            out.push(' ');
            i += 2;
            continue;
        }
        if b == b'"' {
            in_string = true;
            out.push(' ');
            i += 1;
            continue;
        }

        out.push(b as char);
        i += 1;
    }
    out
}

/// 取出 `fn <name>` 的函数体（含大括号）文本；找不到返回 `None`。
///
/// 从 `fn <name>(` 之后第一个 `{` 开始做花括号配对。输入须是已经过
/// [`blank_out_comments_and_strings`] 处理的文本（否则字符串里的花括号会提前收口）。
fn extract_fn_body(src: &str, name: &str) -> Option<String> {
    let needle = format!("fn {name}(");
    let start = src.find(&needle)?;
    let open = src[start..].find('{')? + start;
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(src[open..=i].to_string());
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// 源码第 `offset` 字节偏移所在的 1-based 行号（用于把违规点报成人能定位的位置）
fn line_of(src: &str, offset: usize) -> usize {
    src[..offset.min(src.len())].matches('\n').count() + 1
}

/// 找出 `body` 内每一处 `tracing::debug!` 的 1-based 行号（相对 `body` 起始行）
fn debug_log_lines(body: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = body[from..].find("tracing::debug!") {
        let at = from + rel;
        hits.push(body[..at].matches('\n').count() + 1);
        from = at + 1;
    }
    hits
}

// ==================== 站点登记（宿主热路径，逐条附触发形状） ====================

/// 被锁函数：`src-relative-path` + 函数名 + 为什么它是热路径
///
/// **路径可以是宿主 `src/` 以外的位置**：热路径随能力域 crate 化（wasm-core-lib-split
/// 票 03/04）后会离开 `src-tauri/src`，锁必须跟着走，否则「函数不在登记里」会被
/// 读成「扫描器失效」。WS 帧路径现居
/// `packages/bedcode-server-websocket/src/plugin_binding.rs`。
const LOCKED_SITES: &[(&str, &str, &str)] = &[
    (
        // wasm-core-whole-crate：wasm_core 整核迁入 `bedcode-wasm-core` crate，
        // 热路径随迁，锁路径指向 crate 内文件（相对 src-tauri 根）
        "../packages/bedcode-wasm-core/src/bus.rs",
        "dispatch_publish",
        "按消息触发：PTY 输出通知 / WS 事件流等数据面 topic 走这里",
    ),
    (
        "../packages/bedcode-wasm-core/src/security/fs_auth.rs",
        "check",
        "按文件操作触发：插件每次读写都过一次放行判定",
    ),
    (
        "../packages/bedcode-server-websocket/src/plugin_binding.rs",
        "record_dropped_frame",
        "按帧触发：未导出 events-ws 的插件每条 WS 帧都到这里",
    ),
];

/// 宿主根目录（`bedcode-desktop/src-tauri`）
fn desktop_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_source(rel: &str) -> String {
    let path = desktop_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()))
}

// ==================== 锁：C-001 / C-002 / C-003 ====================

/// C-001 / C-002 / C-003：热路径函数体内不得出现 `tracing::debug!`
#[test]
fn hot_path_functions_do_not_log_per_operation() {
    let mut violations: Vec<String> = Vec::new();

    for (rel, func, why) in LOCKED_SITES {
        let path = desktop_root().join(rel);
        assert!(
            path.is_file(),
            "站点文件缺失：{} —— 防回接锁失效（该热路径已无锁覆盖）",
            path.display()
        );
        let raw = read_source(rel);
        let masked = blank_out_comments_and_strings(&raw);

        let Some(body) = extract_fn_body(&masked, func) else {
            violations.push(format!(
                "{rel}::{func} —— 函数体未找到（改名 / 移动后本锁失效，必须同步更新 LOCKED_SITES）"
            ));
            continue;
        };
        // 函数体在 masked 文本里的起始偏移：body 之前的前缀长度
        let body_start = masked.find(&body).unwrap_or(0);
        let abs_start = line_of(&masked, body_start);

        for rel_line in debug_log_lines(&body) {
            violations.push(format!(
                "{rel}:{}（fn {func} 内第 {rel_line} 行）—— {why}",
                abs_start + rel_line - 1
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "热路径逐次日志被回接（AGENTS §8 日志红线②：PTY 输出 / WS 每帧不打逐帧日志）：\n  {}\n\n\
         这些站点按消息 / 按帧 / 按文件操作触发，逐条 debug 会把日志冲成噪声并淹没真正的 warn / error。\n\
         需要留痕时按下列口径改，不要加回 debug：\n\
         1) 正常路径不打日志，只在异常分支 warn / error（队列满、格式不匹配、拒绝、失败）；\n\
         2) 计数型可观测性走 core-monitor 指标（PluginMetrics），不靠日志——\n\
            bus 有 dropped / format_rejected，ws 有 dropped_frame_count，授权决策有 record_authz_decision；\n\
         3) 确实要留痕就降频到「首次 warn + 累计量」，参照 host_api::ws::record_dropped_frame 的 warn-once 形状。",
        violations.join("\n  ")
    );
}

// ==================== 锁自身的有效性（C-004：防空转） ====================

/// C-004 反例：扫描器必须真的能发现违规——否则上面的锁可能「因为什么都没扫到」
/// 而恒过（vacuous pass），那是比没有锁更坏的结果。
///
/// 两条独立证据：
/// 1. planted fixture：函数体内植入 `tracing::debug!` → 必须被指出行号；
/// 2. 真实文件反例：把某个锁定站点的函数名指向**确实带 debug 的真实函数** →
///    必须报出违规（证明扫描器跑在真实文件上也能命中，不是只对 fixture 有效）。
#[test]
fn scanner_detects_planted_and_real_violations() {
    // --- 1. planted fixture ---
    let planted = r#"
        fn dispatch_publish(&self, topic: &str) {
            let x = 1;
            tracing::debug!("MessageBus: planted {}", topic);
            let y = 2;
        }
    "#;
    let masked = blank_out_comments_and_strings(planted);
    let body = extract_fn_body(&masked, "dispatch_publish").expect("planted fixture 必须能抽出函数体");
    let hits = debug_log_lines(&body);
    assert_eq!(
        hits.len(),
        1,
        "植入的 tracing::debug! 必须被恰好命中一次，实际命中 {hits:?}"
    );

    // --- 1b. 反例：注释与字符串里的 debug 字样不算违规（否则会被注释绕过）---
    let commented = r##"
        fn dispatch_publish(&self) {
            // tracing::debug!(this is a note, not a log call);
            let note = "tracing::debug!(inside a string literal)";
            let _ = note;
        }
    "##;
    let masked_c = blank_out_comments_and_strings(commented);
    let body_c = extract_fn_body(&masked_c, "dispatch_publish").expect("fixture 抽出函数体");
    assert!(
        debug_log_lines(&body_c).is_empty(),
        "注释 / 字符串字面量里的 debug 字样不得算违规，实际命中 {:?}",
        debug_log_lines(&body_c)
    );

    // --- 2. 真实文件反例 ---
    // `plugin_binding::dispatch_frame` 不在 LOCKED_SITES 里，且其函数体确实带
    // `tracing::debug!`（同名函数名一经重命名即失效，见步骤 2b）；把它当锁定站点
    // 扫，必须报违规——证明扫描器跑在真实文件上也能命中，不是只对 fixture 有效。
    let raw = read_source("../packages/bedcode-server-websocket/src/plugin_binding.rs");
    let masked = blank_out_comments_and_strings(&raw);
    let real_body = extract_fn_body(&masked, "dispatch_frame").expect("真实文件里必须能抽出 dispatch_frame 函数体");
    assert!(
        !debug_log_lines(&real_body).is_empty(),
        "扫描器对真实文件失效：dispatch_frame 的已知 debug 未被命中 —— C-004 反例失去证据力"
    );

    // --- 3. 对照：真实锁定站点现在必须干净（否则上面的「反例」就没有区分力）---
    let (_, clean_func, _) = LOCKED_SITES[2];
    let clean_body = extract_fn_body(&masked, clean_func).expect("抽出真实锁定站点函数体");
    assert!(
        debug_log_lines(&clean_body).is_empty(),
        "{clean_func} 当前仍有 tracing::debug! —— C-004 的正反对照同时失效，先修站点再谈锁"
    );
}
// ==================== 解析原语的边界用例（C-005） ====================

/// C-005：花括号配对不能被字符串 / 注释里的括号带偏，且偏移必须可回溯到原文行号
#[test]
fn fn_body_extraction_survives_braces_in_literals_and_tracks_lines() {
    let src = r##"// fn outer_placeholder() { 这行注释里的花括号不得干扰配对 }
fn demo(a: u32) -> u32 {
    let s = "}}} 这串括号在字符串里";
    let t = r#"{{ 原始字符串里的花括号 }}"#;
    let u = /* 块注释里的 { { { */ a;
    tracing::debug!("x={} y={} z={}", s, t, u);
    if a == 0 { return 0; }
    u + 1
}
fn tail() -> u32 { 0 }
"##;
    let masked = blank_out_comments_and_strings(src);
    let body = extract_fn_body(&masked, "demo").expect("必须抽出 demo 函数体");

    // 边界：demo 的函数体必须恰好终止在 demo 自身的收尾大括号，
    // 不能吞掉后面的 `fn tail`（否则配对被字符串里的 `}}}` 带偏）
    assert!(
        !body.contains("fn tail"),
        "函数体越界吞掉了后续函数 —— 花括号配对被字面量里的括号带偏：\n{body}"
    );
    assert!(
        body.contains("tracing::debug!"),
        "函数体截断过早，丢了末尾语句：\n{body}"
    );

    // 偏移可回溯：demo 的 debug 行号必须等于它在原文里的真实行号
    let body_start = masked.find(&body).expect("body 在 masked 中定位");
    let abs_start = line_of(&masked, body_start);
    let hits = debug_log_lines(&body);
    assert_eq!(hits.len(), 1, "demo 体内应恰好一处 debug");
    let reported = abs_start + hits[0] - 1;
    let real_line = src
        .lines()
        .position(|l| l.contains("tracing::debug!"))
        .expect("原文含该 debug 行")
        + 1;
    assert_eq!(
        reported, real_line,
        "上报行号必须等于原文行号（偏移需可回溯定位），reported={reported} real={real_line}"
    );
}
