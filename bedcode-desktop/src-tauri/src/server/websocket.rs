//! WebSocket 传输面
//!
//! 连接骨架（`conn`）、三通道实现（`channel`）、连接/端点注册表（`registry` /
//! `endpoint`）、输出订阅原语（`subscription`）、移动端兼容 wire 协议（`message`）、
//! WsSession 连接态（`session`）、生命周期与优雅停机（`websocket_manager`）、
//! 终端输出端子面（`terminal_ws`）与路由装配
//! （`routes`：三条握手端点 + 帧上限）。
//!
//! `services` 承载的会话控制与终端输入**不是 WS 传输原语**（ADR 0022 裁剪线视角，
//! 归属应为会话业务、后续下沉插件线）——本目录只是它的临时住处，见该模块注释。
//! 依赖方向（不变量 I2）：只**向下**依赖 [`crate::server::core`]，与 `http` 面零横向
//! import（I1，票 08 加锁）。认证档位词汇 `EndpointAuth` 直连桌面 SDK
//! （`bedcode_plugin_api`），不建共享词汇模块。

pub mod channel;
pub mod conn;
pub mod endpoint;
pub mod message;
pub mod registry;
pub mod routes;
pub mod services;
pub mod session;
pub mod subscription;
pub mod terminal_ws;
pub mod websocket_manager;

pub use routes::configure_routes;
pub use websocket_manager::{ClientSummary, ServerEvent, WebSocketManager};

// ==================== 依赖方向结构锁（票 08） ====================

/// 钉死 spec D7 三不变量的源码文本锁（一处锁两侧）
///
/// - **I1** `http` ↮ `websocket` 双向零横向 import（断言 A：跨面绝对/相对路径）
/// - **I2** 两面只向下依赖 `core`（由 A + 断言 B 的旧路径禁令共同兜住）
/// - **I3** `core → 传输面` 仅 `core/app.rs` 组合物豁免，且引用逐条在白名单内
///   （断言 C；`supervisor` / `filter` 的已知豁免见 [`CORE_TRANSPORT_FACE_ALLOWLIST`]，
///   偏离票面「除 app.rs 外零引用」字面的原因写在该常量注释与票 08 Comments）
///
/// 手法沿用本仓先例（`gateway.rs` 路由锁 / `pty_e2e.rs` 接线锁的源码文本断言）；
/// 因需覆盖**动态文件清单**，改用 `CARGO_MANIFEST_DIR` + 递归 `read_dir`——硬编码
/// 清单会让下一个新增文件绕过锁。
///
/// 本模块挂在入口文件 `websocket.rs` 而非 `websocket/` 目录内：扫描范围是
/// `src/server/websocket/**`（目录内容），入口在目录之外——锁正文里的禁用字面量
/// （如 `"server::http"`）不会被断言 A/B 扫回自身造成自锁。测试只做文本扫描，
/// **不 import `http` 面**（类型系统引用会自毁断言 A）。
#[cfg(test)]
mod dependency_direction_lock {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};

    /// `core/` 侧允许出现的传输面引用全集：`(相对 src/server/ 的文件, 允许路径)`
    ///
    /// 「多一条即红」的可审计清单（D7 I3）。每条都必须在扫描中至少命中一次，
    /// 死条目（改名/删除后白名单未跟上）同样报红——双向比对防清单腐烂。
    ///
    /// 三段来源：
    /// 1. `core/app.rs`——票面 D7/I3 组合物豁免，三条（两侧 `configure_routes` +
    ///    `TrafficFilter`；后者挂 `App` 级是 §1.4 红线，收进 `/api` scope 另票）。
    /// 2. `core/supervisor.rs`——**已知豁免，票面字面偏离**：bootstrap 所有权倒挂
    ///    （spec 病灶 4 / §9.2）登记的 `supervisor → WebSocketManager` 活代码，
    ///    本任务 move-only 明确不修；抽 `core::server_runtime` 正名后应整段删掉
    ///    本条，并把断言 C 收回「除 app.rs 外零引用」。
    /// 3. `core/filter.rs`——**已知豁免，票面字面偏离**：票 04 为修 `cargo doc`
    ///    断链写下的 rustdoc 绝对路径内链（`filter.rs:53`），非代码 import；
    ///    若未来改成不含 `server::http` 的写法，删本条即可。
    const CORE_TRANSPORT_FACE_ALLOWLIST: &[(&str, &[&str])] = &[
        (
            "core/app.rs",
            &[
                "crate::server::http::configure_routes",
                "crate::server::websocket::configure_routes",
                "crate::server::http::middleware::http_filter::TrafficFilter",
            ],
        ),
        (
            "core/supervisor.rs",
            &[
                "crate::server::websocket::WebSocketManager",
                "crate::server::websocket::ServerEvent",
                "crate::server::websocket::registry::WsSessionRegistry",
            ],
        ),
        (
            "core/filter.rs",
            &["crate::server::http::middleware::http_filter"],
        ),
    ];

    /// 断言 B 禁止复发的旧平铺路径子段（票 08 断言 B 原文清单）
    ///
    /// 这些名字现在必须带 `core::` / `http::` / `websocket::` 中间层；
    /// 形态判据是 `server::{子段}` 且子段后非标识符字节（`server::ws` 不会
    /// 误配 `server::websocket`）。
    const LEGACY_SERVER_CHILD_SEGMENTS: &[&str] = &[
        "controllers",
        "dtos",
        "gateway",
        "middleware",
        "services",
        "ws",
        "app",
        "message",
        "connection_types",
        "filter",
        "metrics",
        "link_crypto",
        "supervisor",
        "port_checker",
    ];

    /// 三个面目录的非空哨兵：枚举失败/路径写错时防空转（全空则断言恒真）
    const FACE_SENTINELS: &[(&str, &str)] = &[
        ("http", "http/gateway.rs"),
        ("http", "http/routes.rs"),
        ("websocket", "websocket/message.rs"),
        ("websocket", "websocket/routes.rs"),
        ("core", "core/app.rs"),
        ("core", "core/supervisor.rs"),
    ];

    /// 各面 `.rs` 文件数下限（2026-09-23 实测 14 / 20 / 6，取整下调留余量）
    const MIN_HTTP_FILES: usize = 10;
    const MIN_WEBSOCKET_FILES: usize = 10;
    const MIN_CORE_FILES: usize = 5;

    fn is_ident_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    fn server_src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server")
    }

    /// 递归枚举 `src/server/{face}/**/*.rs`，返回相对 `src/server` 的正斜杠路径
    ///
    /// **必须动态枚举**（票 08）：硬编码清单会让新增文件绕过锁。
    fn collect_rs_files(face: &str) -> Vec<String> {
        let root = server_src_root().join(face);
        let mut stack = vec![root];
        let mut out = Vec::new();
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("结构锁无法枚举 {}：{e}", dir.display()));
            for entry in entries {
                let entry = entry.unwrap_or_else(|e| panic!("结构锁 read_dir 条目错误：{e}"));
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    let rel = path
                        .strip_prefix(server_src_root())
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(rel);
                }
            }
        }
        out.sort();
        out
    }

    fn read_source(rel: &str) -> String {
        let abs = server_src_root().join(rel);
        std::fs::read_to_string(&abs)
            .unwrap_or_else(|e| panic!("结构锁无法读取 {}：{e}", abs.display()))
    }

    /// 1-based 行号（`src[..pos]` 内换行数 + 1，CRLF 的 `\r` 不影响计数）
    fn line_number(src: &str, pos: usize) -> usize {
        src[..pos].bytes().filter(|b| *b == b'\n').count() + 1
    }

    /// 取 `pos` 所在行原文（去掉行尾 `\r`，失败消息可读）
    fn line_content(src: &str, pos: usize) -> String {
        let start = src[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = src[pos..].find('\n').map(|i| pos + i).unwrap_or(src.len());
        src[start..end].trim_end_matches('\r').to_string()
    }

    /// `seg_start` 处段名的前一段名（`seg_start` 指向段首字节，其前应为 `::`）
    ///
    /// 用字节比较判 `::`：`start - 2` 在非 `::` 前缀时可能落在 UTF-8 多字节
    /// 字符中间，`&src[a..b]` 切片会直接 panic（2026-09-23 实测中文注释踩中）。
    fn previous_segment<'a>(src: &'a str, seg_start: usize) -> Option<&'a str> {
        let bytes = src.as_bytes();
        if seg_start < 2 || bytes[seg_start - 2] != b':' || bytes[seg_start - 1] != b':' {
            return None;
        }
        let mut j = seg_start - 2;
        while j > 0 && is_ident_byte(bytes[j - 1]) {
            j -= 1;
        }
        if j == seg_start - 2 {
            return None;
        }
        // j 必在 ASCII 标识符/`::` 链上，是合法 char boundary
        src.get(j..seg_start - 2)
    }

    /// 找独立段 `segment` 的出现位置：前为 `::`、后非标识符字节
    fn find_segment_positions(src: &str, segment: &str) -> Vec<usize> {
        let bytes = src.as_bytes();
        let mut positions = Vec::new();
        let mut search = 0usize;
        while let Some(rel) = src[search..].find(segment) {
            let start = search + rel;
            let end = start + segment.len();
            let before_ok =
                start >= 2 && bytes[start - 2] == b':' && bytes[start - 1] == b':';
            let after_ok = bytes.get(end).map_or(true, |&b| !is_ident_byte(b));
            if before_ok && after_ok {
                positions.push(start);
            }
            search = start + 1;
        }
        positions
    }

    /// 断言 A：跨面引用命中清单 `（文件, 行号, 行原文）`
    ///
    /// 判据（段边界，非裸子串）：在 `face` 目录内找段名 `other_face`，且其**前一段**
    /// 是 `server`（绝对 `crate::server::websocket::…`）或 `super`（相对
    /// `super::super::websocket::…`）。`actix_web::http` 前一段是 `actix_web`，
    /// 不误配；`::http_filter` 后随 `_` 不成段。
    fn find_cross_face_refs(files: &[String], other_face: &str) -> Vec<(String, usize, String)> {
        let mut hits = Vec::new();
        for rel in files {
            let src = read_source(rel);
            for seg_start in find_segment_positions(&src, other_face) {
                let prev = previous_segment(&src, seg_start);
                if prev == Some("server") || prev == Some("super") {
                    hits.push((
                        rel.clone(),
                        line_number(&src, seg_start),
                        line_content(&src, seg_start),
                    ));
                }
            }
        }
        hits
    }

    /// 断言 B：旧平铺路径 `server::{LEGACY}` 命中清单（同前 A 的失败形态）
    fn find_legacy_paths(files: &[String]) -> Vec<(String, usize, String)> {
        let mut hits = Vec::new();
        for rel in files {
            let src = read_source(rel);
            for child in LEGACY_SERVER_CHILD_SEGMENTS {
                let needle = format!("server::{child}");
                let mut search = 0usize;
                while let Some(pos_rel) = src[search..].find(&needle) {
                    let start = search + pos_rel;
                    let after = start + needle.len();
                    // `server` 前非标识符（挡 `my_server::ws`），`{child}` 后非标识符
                    // （挡 `server::ws` 配 `server::websocket`——后者 `ws` 后是 `ocket`）
                    let before_ok =
                        start == 0 || !is_ident_byte(src.as_bytes()[start - 1]);
                    let after_ok = src
                        .as_bytes()
                        .get(after)
                        .map_or(true, |&b| !is_ident_byte(b));
                    if before_ok && after_ok {
                        hits.push((
                            rel.clone(),
                            line_number(&src, start),
                            line_content(&src, start),
                        ));
                    }
                    search = start + 1;
                }
            }
        }
        hits
    }

    /// 从 `needle`（如 `server::http`）提取完整传输面引用路径
    ///
    /// - 向前：扩展 `::段`；段首为大写（类型名）则纳入并停止——
    ///   `WebSocketManager::global` 记为 `…::WebSocketManager`，
    ///   `ServerEvent::Started` 记为 `…::ServerEvent`，白名单按**面内定义项**
    ///   枚举，而不是把每个 match 至变体钉进清单。
    /// - 边界：`server::http` 后随标识符字节（`http_filter` / `websocket_manager`
    ///   直接粘连）不算命中；`server::http::` / `server::http)` 算。
    /// - 向后：扩展 `::标识符` 收全 `crate::` 前缀。
    fn extract_face_paths(src: &str, needle: &str) -> Vec<(usize, String)> {
        let mut results = Vec::new();
        let mut search = 0usize;
        while let Some(rel) = src[search..].find(needle) {
            let start = search + rel;
            let after = start + needle.len();
            let before_ok = start == 0 || !is_ident_byte(src.as_bytes()[start - 1]);
            let suffix_ok = src
                .as_bytes()
                .get(after)
                .map_or(true, |&b| !is_ident_byte(b));
            if before_ok && suffix_ok {
                let mut pstart = start;
                while pstart >= 2
                    && src.as_bytes()[pstart - 2] == b':'
                    && src.as_bytes()[pstart - 1] == b':'
                {
                    let mut j = pstart - 2;
                    while j > 0 && is_ident_byte(src.as_bytes()[j - 1]) {
                        j -= 1;
                    }
                    if j == pstart - 2 {
                        break;
                    }
                    pstart = j;
                }
                let mut end = after;
                while let Some(rest) = src.get(end..) {
                    if let Some(stripped) = rest.strip_prefix("::") {
                        let seg_len = stripped
                            .bytes()
                            .take_while(|b| is_ident_byte(*b))
                            .count();
                        if seg_len == 0 {
                            break;
                        }
                        let seg = &stripped[..seg_len];
                        end = end + 2 + seg_len;
                        if seg.starts_with(|c: char| c.is_ascii_uppercase()) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                let path = format!("{}{}", &src[pstart..start], &src[start..end]);
                results.push((start, path));
            }
            search = start + 1;
        }
        results
    }

    fn format_hits(hits: &[(String, usize, String)]) -> String {
        hits.iter()
            .map(|(f, l, c)| format!("  {f}:{l}: {c}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 非空哨兵：枚举失败/路径写错时防「全空清单 → 断言恒真」
    fn assert_face_enumeration_is_live(files_by_face: &[(&str, Vec<String>)]) {
        for (face, files) in files_by_face {
            let min = match *face {
                "http" => MIN_HTTP_FILES,
                "websocket" => MIN_WEBSOCKET_FILES,
                "core" => MIN_CORE_FILES,
                other => panic!("未登记的面目录 {other}，更新哨兵"),
            };
            assert!(
                files.len() >= min,
                "结构锁空转：src/server/{face}/ 只枚举到 {} 个 .rs（下限 {min}）——\
                 路径错或 read_dir 异常被吞",
                files.len()
            );
        }
        for (face, sentinel) in FACE_SENTINELS {
            let files = files_by_face
                .iter()
                .find(|(f, _)| f == face)
                .unwrap_or_else(|| panic!("缺少面目录 {face} 的枚举结果"));
            assert!(
                files.1.iter().any(|p| p == sentinel),
                "结构锁空转：{face}/ 枚举清单缺少哨兵文件 {sentinel}（实际 {} 个文件）",
                files.1.len()
            );
        }
    }

    /// 锁本体：I1 跨面零引用 + 旧路径不复发 + core 白名单双向比对
    ///
    /// 契约（票 08）：
    /// - C-A：`http` 侧无指向 `websocket` 的绝对/相对路径；反向同理——正例由
    ///   变异自检证明（临时加 `use crate::server::websocket::…` 必须红）。
    /// - C-B：两面无 `server::{legacy}::` 旧平铺路径（14 个子段全表）。
    /// - C-C：`core/` 传输面引用路径集合 == 白名单集合（多一条红、少一条/死条目也红）。
    /// - C-D：枚举非空且含哨兵（防空转恒真）。
    #[test]
    fn http_and_websocket_are_independent() {
        let http_files = collect_rs_files("http");
        let ws_files = collect_rs_files("websocket");
        let core_files = collect_rs_files("core");
        assert_face_enumeration_is_live(&[
            ("http", http_files.clone()),
            ("websocket", ws_files.clone()),
            ("core", core_files.clone()),
        ]);

        // ---- 断言 A：双向零横向引用 ----
        let http_to_ws = find_cross_face_refs(&http_files, "websocket");
        assert!(
            http_to_ws.is_empty(),
            "I1 违反：http/ 侧出现指向 websocket 面的路径（前一段为 server 或 super）：\n{}",
            format_hits(&http_to_ws)
        );
        let ws_to_http = find_cross_face_refs(&ws_files, "http");
        assert!(
            ws_to_http.is_empty(),
            "I1 违反：websocket/ 侧出现指向 http 面的路径（前一段为 server 或 super）：\n{}",
            format_hits(&ws_to_http)
        );

        // ---- 断言 B：旧平铺路径不得复发 ----
        let legacy = find_legacy_paths(&http_files)
            .into_iter()
            .chain(find_legacy_paths(&ws_files));
        let legacy: Vec<_> = legacy.collect();
        assert!(
            legacy.is_empty(),
            "旧路径复发（须带 core:: / http:: / websocket:: 中间层）：\n{}",
            format_hits(&legacy)
        );

        // ---- 断言 C：core/ 传输面引用 == 白名单（双向、逐条） ----
        let mut allowed_by_file: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (file, paths) in CORE_TRANSPORT_FACE_ALLOWLIST {
            let set = allowed_by_file
                .entry(file)
                .or_default();
            for p in *paths {
                set.insert(*p);
            }
        }

        let mut found_by_file: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut overkill: Vec<String> = Vec::new();
        for rel in &core_files {
            let src = read_source(rel);
            let mut paths_here: BTreeSet<String> = BTreeSet::new();
            // (行首字节偏移, 路径) —— 失败消息要能直接定位到行
            let mut located: Vec<(usize, String)> = Vec::new();
            for needle in ["server::http", "server::websocket"] {
                for (pos, path) in extract_face_paths(&src, needle) {
                    paths_here.insert(path.clone());
                    located.push((pos, path));
                }
            }
            if paths_here.is_empty() {
                continue;
            }
            let allowed = allowed_by_file.get(rel.as_str());
            for path in &paths_here {
                let ok = allowed
                    .map(|s| s.contains(path.as_str()))
                    .unwrap_or(false);
                if !ok {
                    let pos = located
                        .iter()
                        .find(|(_, p)| p == path)
                        .map(|(p, _)| *p)
                        .unwrap_or(0);
                    overkill.push(format!(
                        "{rel}:{}: 非白名单传输面引用 `{path}`（行：{}）",
                        line_number(&src, pos),
                        line_content(&src, pos)
                    ));
                }
            }
            found_by_file.insert(rel.as_str(), paths_here);
        }
        assert!(
            overkill.is_empty(),
            "I3 违反：core/ 传输面引用超出白名单（多一条即红；supervisor/filter 的已知豁免\
             见 CORE_TRANSPORT_FACE_ALLOWLIST，新增引用须先裁决再进清单）：\n  {}",
            overkill.join("\n  ")
        );

        // 反向：白名单死条目（引用已删/改名但清单未收回）
        let mut dead: Vec<String> = Vec::new();
        for (file, allowed) in allowed_by_file {
            if !core_files.iter().any(|p| p == file) {
                dead.push(format!("{file}: 白名单文件已不在 core/ 枚举清单（路径改名？）"));
                continue;
            }
            let found = found_by_file.get(file).cloned().unwrap_or_default();
            for p in allowed {
                if !found.contains(p) {
                    dead.push(format!(
                        "{file}: 白名单死条目 `{p}`（扫描零命中——引用已删或提取规则漂移）"
                    ));
                }
            }
        }
        assert!(
            dead.is_empty(),
            "白名单与实际引用不一致（须双向同步）：\n  {}",
            dead.join("\n  ")
        );
    }

    /// 提取规则自身的契约例（防锁的实现被改坏后假绿）
    ///
    /// 正例：crate 前缀收全、大写段截断、`http_filter` 粘连边界不误配。
    /// 反例：无 `server::` 核的文档相对写法、无前缀裸段不产出。
    #[test]
    fn face_path_extraction_follows_uppercase_stop_and_boundaries() {
        let src = "\
let a = crate::server::http::configure_routes(cfg);\n\
let b = crate::server::websocket::WebSocketManager::global();\n\
let c = crate::server::websocket::ServerEvent::Started;\n\
let d = crate::server::http::middleware::http_filter::TrafficFilter;\n\
//! [crate::server::http::middleware::http_filter]\n\
use crate::server::http_filter::No;\n\
use actix_web::http::header::X;\n";
        let http_paths: Vec<_> = extract_face_paths(src, "server::http")
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        assert_eq!(
            http_paths,
            vec![
                "crate::server::http::configure_routes".to_string(),
                "crate::server::http::middleware::http_filter::TrafficFilter".to_string(),
                "crate::server::http::middleware::http_filter".to_string(),
                // 第四处是 server::http_filter 粘连——suffix 边界必须挡下
            ],
            "server::http 提取清单漂移"
        );
        let ws_paths: Vec<_> = extract_face_paths(src, "server::websocket")
            .into_iter()
            .map(|(_, p)| p)
            .collect();
        assert_eq!(
            ws_paths,
            vec![
                "crate::server::websocket::WebSocketManager".to_string(),
                // ServerEvent::Started 停在 ServerEvent（大写段截断）
                "crate::server::websocket::ServerEvent".to_string(),
            ],
            "server::websocket 提取清单漂移"
        );

        // actix_web::http 不带 server 核：extract 按 needle 不命中；
        // 跨面判据的前一段规则再钉一次（不走文件 I/O）
        assert_eq!(previous_segment("xx::http::y", 4), Some("xx"));
        assert_eq!(previous_segment("crate::server::http::y", 15), Some("server"));
        assert_eq!(previous_segment("super::http::y", 7), Some("super"));
        // 非 `::` 前缀（含 UTF-8 多字节相邻）不得 panic、不得误判
        assert_eq!(previous_segment("（http", 3), None);
        assert_eq!(previous_segment("http", 0), None);
    }
}
