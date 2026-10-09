//! 能力域 crate 的**语义**防回接锁：产品插件 id 与退役宿主面词汇不得进生产代码
//!
//! ## 为什么已有锁不够
//!
//! 仓库里的边界锁全是**结构锁**：`crate_boundary_lock`（依赖边登记表 + 双向断言）、
//! 各传输面的 `dependency_direction_lock`（禁止横向边）、`HostModuleDesc` 的产品名词
//! 词段锁。它们管的是「边是否合法」「描述符词汇是否干净」——但**没有一条管合法边
//! 位置上装的是不是业务代码**。于是一条 `com.bedcode.terminal-session` 字面量写在
//! 能力 crate 里能通过全部现有测试绿灯通过。
//!
//! 本锁补的就是这个缺口：**扫描能力域 crate 的生产代码文本**，命中即红。
//!
//! ## 扫描面与排除面（排除必须显式、可审计）
//!
//! - **扫**：`bedcode-*` 能力域 / 传输面 crate 的 `src/`（9 个，均在**仓库根
//!   `packages/`**，2026-10-07 从 `bedcode-desktop/packages/` 迁根；`bedcode-ws-client-engine`
//!   随 ADR 0043 抽根入面）。
//! - **不扫注释**：`//`、`///`、`//!` 一律剥掉。注释里点名产品（解释「为什么」）是
//!   正常且必要的，把注释当代码判会产出噪音锁，噪音锁会被忽略，忽略的锁等于没有。
//! - **不扫测试区**：路径含 `tests` 段的独立测试文件、以及文件内首个
//!   `#[cfg(...test...)]` 之后的内容。黄金形状锁、回归夹具按设计要复刻产品字节。
//! - **不扫 `bedcode-wasm-core`**：该 crate 仍有产品 id 泄漏（`security/fs_auth.rs`
//!   的 `FIRST_PARTY_TRUSTED_DIRS` 第一方免弹窗豁免表、`manager/host/activation.rs`
//!   的 `LEGACY_API_PLUGIN_ALIASES` 退役别名表）由独立票据处置，处理完再把 crate 名
//!   加进下方 `SCANNED_CRATES`。原先同在此列的 `utils/session_gateway.rs` 硬编码互调
//!   api 名与 `utils/auth/auth_center.rs::SESSION_PLUGIN_ID` 已随宿主薄壳回迁 lib 处置
//!   完毕——本行据实维护，处置进度不靠记忆。
//!
//! ## 两类判据（AGENTS §5.1 B1/B5 与 §5.3 已退役面）
//!
//! 1. **产品插件 id**：`com.bedcode.<产品段>` 字面量。插件 id 归内核，但**具体某个
//!    产品插件的 id** 是产品事实——能力 crate 里出现它就说明该 crate 知道了「有哪
//!    些产品、各做什么」，直接损害「任何宿主可复用」这个属性。
//! 2. **退役宿主面词汇**：`host-session` / `host-terminal` / `output-ring-fetch` 等。
//!    AGENTS §5.3 已把它们连同权限位整体退役；即便形态是常量或注释里的引用名，
//!    留着也是回接磁铁。
//!
//! ## 唯一的登记例外，以及它为什么合法
//!
//! `bedcode-server-http` 的 `LEGACY_HTTP_PLUGIN_ALIASES`（退役插件 id → 接管方 id）。
//! 它归类为 AGENTS §5.1.3 ③「通用注册表与寻址」：判据是 **id 对 id 的寻址**，不含
//! 产品语义——不描述会话 / 终端 / 传输的业务含义，也不替插件决定业务上该怎样，接管方
//! 插件自己注册路由自己应答。插件身份属内核面，故退役 id 的接管关系属内核可持有的表。
//!
//! 本锁对它做**内容钉死**而非整文件放行：登记表里写死它当前的全部产品 id 字面量，
//! 表里多一条、少一条、改一条接管关系，全部测红——改动必须回到这张锁前说明理由。

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

// ==================== 登记表（唯一事实源） ====================

/// 纳入扫描的能力域 / 传输面 crate（crate 名；落点见 [`crate_dir`]）
///
/// 新增能力 crate 必须登记进来——**不登记 = 不受本锁管辖**，故登记动作本身是
/// 「承认它进入语义管辖范围」的一次显式决定。
const SCANNED_CRATES: &[&str] = &[
    "bedcode-server-base",
    "bedcode-crypto-engine",
    "bedcode-server-core",
    "bedcode-server-http",
    "bedcode-server-websocket",
    "bedcode-server-peer-net",
    "bedcode-discovery-engine",
    // 自 wasm-core 迁出的 pty 引擎面（引擎无产品语义：PTY 生命周期 / 环形缓冲 /
    // 特殊键 wire 形状）。迁出即入扫描面——新 crate 的默认状态就该受管辖。
    "bedcode-pty-engine",
    // 自移动端 fork crate 抽根的 WS 出站连接引擎（ADR 0043）：句柄表 / 心跳 /
    // 退避重连 / 帧信封，全是传输机制，无任何产品语义。与上者同理——抽根即入扫描面。
    "bedcode-ws-client-engine",
];

/// 已知的、暂未纳入扫描的 `bedcode-*` crate（各有明确在办票据）
///
/// 与 `SCANNED_CRATES` 合起来构成**完整覆盖**：两个根目录下每个 `bedcode-*`
/// 目录必须在两者之一里（C-3 反向断言）。没有这个桶，「把 crate 从登记表里删掉」
/// 就是一条零成本后门——锁照样绿，覆盖面悄悄少一块。进本桶同样不是免费的：处置
/// 完成后删掉本条目即自动进入扫描面。
const PENDING_SCAN_CRATES: &[(&str, &str)] = &[
    (
        "bedcode-wasm-core",
        "机制整核仍有的产品 id 泄漏（fs_auth.rs 的 FIRST_PARTY_TRUSTED_DIRS 第一方豁免表 / \
         activation.rs 的 LEGACY_API_PLUGIN_ALIASES 退役别名表）由独立票据处置，处置完删本条目",
    ),
    (
        "bedcode-host-kit",
        "机制内核（ADR 0035 D1：组件状态 / 能力模块契约 / 自动注册表），不是能力域 crate；\
         它自 2026-10-07 起与能力域 crate 同落仓库根 packages/，本锁过去因分处两端而天然不在\
         扫描面。纳入语义扫描另立票据",
    ),
];

/// 产品 id 形状：反向域名 + 插件 id 段
///
/// 只取 `com.bedcode.` 之后**第一段**作判定：反向域名与 `bedcode` 是组织标识
/// （机制面），紧随其后的段才是产品身份。
const PRODUCT_ID_PREFIX: &str = "com.bedcode.";

/// 格式占位段：出现在**格式校验类**夹具 / 示例里的裸占位不是产品身份
///
/// 只收「明显不是任何真实插件」的中性词。真实产品插件名（terminal-session /
/// agent-hub / file-transfer / ai-chatbox …）一律不在此表——出现在生产代码里就红。
const PLACEHOLDER_SEGMENTS: &[&str] = &["xxx", "test", "other", "example", "sample"];

/// 已退役的宿主面词汇（AGENTS §5.3）：`host-session` / `host-terminal` 连同权限位
/// 整体退役，业务下沉到 `com.bedcode.terminal-session`。任何形态的生产引用都红。
///
/// 后两个是本轮实际删掉的死常量名（原 `bedcode-server-base/src/constants.rs` 的
/// `PLUGIN_SESSION_RING_FETCH_MAX_BYTES` / `ENV_BEDCODE_SESSION_ID`，零消费者）。
/// 列进来是为了让「删掉」变成「删掉且锁住」——否则它们随时能被原样写回。
const RETIRED_HOST_SURFACE_TOKENS: &[&str] = &[
    "host-session",
    "host-terminal",
    "output-ring-fetch",
    "session-status-changed",
    "PLUGIN_SESSION_RING_FETCH_MAX_BYTES",
    "ENV_BEDCODE_SESSION_ID",
];

/// 登记例外：允许存在的产品 id 字面量（按字面量钉死，不是按文件放行）
struct ProductIdException {
    /// 所属 crate（须在 `SCANNED_CRATES` 内）
    crate_name: &'static str,
    /// crate 内相对路径（`/` 分隔）
    file_rel: &'static str,
    /// 归类依据（写进报错文案，评审时看得见理由）
    reason: &'static str,
    /// 该文件当前**全部**产品 id 字面量，按字典序去重；与实测不一致即红
    expected_ids: &'static [&'static str],
}

const PRODUCT_ID_EXCEPTIONS: &[ProductIdException] = &[ProductIdException {
    crate_name: "bedcode-server-http",
    file_rel: "src/controllers/plugin_controller.rs",
    reason: "LEGACY_HTTP_PLUGIN_ALIASES：退役插件 id → 接管方 id 的迁移映射，属 \
             AGENTS §5.1.3 ③ 通用注册表与寻址（id 对 id，不含产品语义）；切断后删除本例外",
    expected_ids: &[
        "com.bedcode.auto-task",
        "com.bedcode.session",
        "com.bedcode.terminal-session",
    ],
}];

// ==================== 扫描器 ====================

/// 宿主根目录（`bedcode-desktop/src-tauri`）
fn desktop_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 桌面端 `packages/`（2026-10-08 起仅剩 `plugin-*` 契约与夹具 crate，无 `bedcode-*`；
/// 保留枚举是为了 C-3 反向断言面不缩水——未来若再落回一个 `bedcode-*` 目录会被抓到）
fn desktop_packages_dir() -> PathBuf {
    desktop_root()
        .parent()
        .map(|p| p.join("packages"))
        .expect("src-tauri 的上级应是 bedcode-desktop")
}

/// 仓库根 `packages/`（机制内核 `bedcode-host-kit`、8 个能力域 / 传输面 crate
/// 自 2026-10-07 起、整核本体 `bedcode-wasm-core` 自 2026-10-08 起落此处；
/// 迁根前能力域 crate 在 `bedcode-desktop/packages/`）
fn repo_packages_dir() -> PathBuf {
    desktop_root()
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("packages"))
        .expect("bedcode-desktop 的上级应是仓库根")
}

/// 扫描面根目录全集（**两个** packages/ 都要枚举，C-3 反向断言才有完整覆盖面）
fn packages_dirs() -> Vec<PathBuf> {
    vec![repo_packages_dir(), desktop_packages_dir()]
}

/// 按 crate 名解析它的目录：逐个根找，**找不到即 panic**——
///
/// 登记表指向一个不存在的目录时，扫描器会静默扫不到任何文件（假绿灯）；
/// 这正是本锁最危险的失守形态，故做成 fail-visible。
fn crate_dir(crate_name: &str) -> PathBuf {
    for root in packages_dirs() {
        let candidate = root.join(crate_name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    panic!(
        "crate `{crate_name}` 在两个根下都找不到（{}）—— crate 改名或移出必须同改登记表，\
         否则扫描器空转、本锁失效",
        packages_dirs()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" / ")
    )
}

/// 逐文件递归收集 `.rs`（目录序稳定，便于报错可复现）
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// 是纯测试文件吗（路径含 `tests` 段：按文件目录化的测试子树）
fn is_test_only_file(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str().to_string_lossy() == "tests")
}

/// 剥掉一行里的**行注释**（引号感知：`//` 出现在字符串或字符字面量内不算注释）
///
/// 引号感知是**必须**的，不是洁癖：本锁是唯一的语义闸门，若朴素的「`//` 之后全丢」
/// 会在 `"http://…"; let id = "com.bedcode.foo"` 这类行上把后面的真命中丢掉，
/// 锁就被静默解除了。逐字节扫描是安全的：UTF-8 多字节序列的续字节都 >= 0x80，
/// 不会与 ASCII 的 `"` `'` `/` 混淆。
///
/// **两个已知边界，都是刻意选的保守方向**（错在「锁红」而非「锁绿」）：
/// - 原始字符串 `r"…"` / `r#"…"#` 内的 `//` 会被当注释切断 —— 该行之后的命中可能
///   漏检。判据：**产品 id 字面量不得与 `//` 同行出现在原始字符串里**。
/// - 块注释 `/* … */` **不剥**（含跨行）。块注释里的产品 id 会照常报出，即假警报。
///   剥错了会静默失守，不剥只会吵 —— 宁吵不静守。
fn strip_line_comments(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    let mut in_str = false;
    let mut in_char = false;
    let mut escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            out.push(b as char);
            escaped = false;
            i += 1;
            continue;
        }
        match b {
            b'\\' if in_str || in_char => {
                out.push(b as char);
                escaped = true;
                i += 1;
            }
            b'"' if !in_char => {
                in_str = !in_str;
                out.push(b as char);
                i += 1;
            }
            b'\'' if !in_str => {
                in_char = !in_char;
                out.push(b as char);
                i += 1;
            }
            b'/' if !in_str && !in_char && i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
            _ => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

/// 该行是否开启 `#[cfg(test…)]` 区（trim 后以 `#[cfg(` 开头且含 `test`）
fn opens_cfg_test(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("#[cfg(") && t.contains("test")
}

/// 一个文件里「生产代码文本」的形态：路径 + 剥注释后的生产行
struct ProdSource {
    path: PathBuf,
    /// crate 内相对路径（报错用）
    rel: String,
    lines: Vec<String>,
}

/// 取一个文件的生产代码（跳过测试文件 / 测试区 / 注释）
fn prod_source(path: &Path, crate_root: &Path) -> Option<ProdSource> {
    if is_test_only_file(path) {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let mut lines = Vec::new();
    for line in text.lines() {
        if opens_cfg_test(line) {
            break;
        }
        lines.push(strip_line_comments(line));
    }
    let rel = path
        .strip_prefix(crate_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    Some(ProdSource {
        path: path.to_path_buf(),
        rel,
        lines,
    })
}

/// 抽产品插件 id 的「第一段」（`com.bedcode.<段>` 的 `<段>`），跳过格式占位段
fn product_segments_in(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(PRODUCT_ID_PREFIX) {
        let after = &rest[at + PRODUCT_ID_PREFIX.len()..];
        let seg: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .collect();
        // 段被 `.` 截断时（如 `com.bedcode.a.b`）只取第一段作身份判定
        let first = seg.split('.').next().unwrap_or("").to_string();
        rest = &after[seg.len()..];
        if first.is_empty() || PLACEHOLDER_SEGMENTS.contains(&first.as_str()) {
            continue;
        }
        found.insert(first);
    }
    found
}

/// 全部产品 id 字面量（重建完整 `com.bedcode.<段>` 形式，用于例外表比对）
fn product_literals_in(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(PRODUCT_ID_PREFIX) {
        let after = &rest[at + PRODUCT_ID_PREFIX.len()..];
        let seg: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .collect();
        rest = &after[seg.len()..];
        if seg.is_empty() {
            continue;
        }
        found.insert(format!("{PRODUCT_ID_PREFIX}{seg}"));
    }
    found
}

/// 扫一个 crate 的全部生产代码
fn scan_crate(crate_name: &str) -> Vec<ProdSource> {
    let crate_root = crate_dir(crate_name);
    let src = crate_root.join("src");
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files);
    files.iter().filter_map(|p| prod_source(p, &crate_root)).collect()
}

/// 取某条例外的实测产品 id 集合（找不到文件 = 空集，交由内容钉死断言报错）
fn observed_literals(src: &[ProdSource], file_rel: &str) -> BTreeSet<String> {
    for s in src {
        if s.rel == file_rel {
            return product_literals_in(&s.lines.join("\n"));
        }
    }
    BTreeSet::new()
}

// ==================== 用例 ====================

/// C-1｜扫描器防空转：必须能认出真命中，且不靠「一刀切丢弃」冒充扫描
///
/// 这条是本锁的自检——锁最危险的失效形态是「扫描器悄悄什么都不匹配」或「扫描器
/// 因为坏掉而永远扫不到东西」，两者都永远绿。四条各自可杀的证明：
///
/// - **C-1a 剥注释不误伤代码**：引号内的 `//` 不是注释，注释后的内容必须被切掉。
/// - **C-1b `cfg(test)` 切区**：`prod_source` 必须真的把测试区截断（否则锁会拿测试
///   夹具里的产品 id 去误判生产代码）。
/// - **C-1c 正负成对**：同一段文本里，注释中的 id 不算命中、代码中的 id 必须算命中。
#[test]
fn scanner_is_not_vacuous() {
    // ---- C-1a：引号感知 ----
    let url_line = strip_line_comments("let u = \"https://a\"; let v = \"com.bedcode.file-transfer\";");
    assert!(
        url_line.contains("com.bedcode.file-transfer"),
        "字符串内的 `//` 不得被当注释起点（否则真命中会被静默丢弃，锁被无声解除），实得 {url_line:?}"
    );
    for (src, must_not_contain, why) in [
        ("let a = 1; // com.bedcode.agent-hub", "agent-hub", "行尾注释"),
        ("// com.bedcode.ai-chatbox", "ai-chatbox", "行首注释"),
    ] {
        let out = strip_line_comments(src);
        assert!(
            !out.contains(must_not_contain),
            "{why}必须被剥掉，否则锁会被文档噪音淹没而被人忽略；实得 {out:?}"
        );
    }
    // 块注释**刻意不剥**：剥不掉的后果是「块注释里的产品 id 被当命中」= 锁红 = 假警报；
    // 若剥了而剥错，后果是「真命中被吞」= 锁绿 = 静默失守。两个错里只有假警报可接受，
    // 故不对称是刻意的。实测七个 crate 的生产代码无独立块注释（`/*` 只出现在已先被
    // `//` 剥掉的文档行内），所以当前零假警报。
    assert!(
        product_segments_in(&strip_line_comments("/* com.bedcode.ai-chatbox */")).contains("ai-chatbox"),
        "块注释内的命中必须**仍然报出**（保守方向：宁可假警报不可静默失守）"
    );

    // ---- C-1b：`cfg(test)` 切区（落真文件，不靠空断言充数）----
    let dir = tempfile::tempdir().expect("临时目录");
    let crate_root = dir.path();
    let src_dir = crate_root.join("src");
    fs::create_dir_all(&src_dir).expect("建 src 目录");
    fs::write(
        src_dir.join("probe.rs"),
        "let live = \"com.bedcode.terminal-session\";\n\
         // com.bedcode.agent-hub\n\
         #[cfg(test)]\n\
         mod tests {\n\
             let dead = \"com.bedcode.ai-chatbox\";\n\
         }\n",
    )
    .expect("写夹具源文件");
    let probe = prod_source(&src_dir.join("probe.rs"), crate_root).expect("真实存在的 .rs 必须能取出生产代码");
    let text = probe.lines.join("\n");

    assert!(
        text.contains("com.bedcode.terminal-session"),
        "生产区的真命中必须被保留，实得 {text:?}"
    );
    assert!(
        !text.contains("ai-chatbox"),
        "`#[cfg(test)]` 之后的测试区必须被截断，否则锁会把测试夹具当成生产代码；实得 {text:?}"
    );
    assert!(!text.contains("agent-hub"), "注释必须被剥掉；实得 {text:?}");
    assert_eq!(
        probe.rel, "src/probe.rs",
        "rel 必须是 crate 内相对路径（报错文案靠它定位），实得 {:?}",
        probe.rel
    );
    // 同理：纯测试文件（路径含 `tests` 段）整个跳过
    let tests_dir = src_dir.join("tests");
    fs::create_dir_all(&tests_dir).expect("建 tests 目录");
    fs::write(tests_dir.join("shapes.rs"), "let _ = \"com.bedcode.git\";").expect("写测试夹具文件");
    assert!(
        prod_source(&tests_dir.join("shapes.rs"), crate_root).is_none(),
        "路径含 `tests` 段的纯测试文件必须整个跳过"
    );
}

/// C-2｜占位段豁免只放行格式夹具，不放行任何真实产品段
#[test]
fn placeholder_segments_do_not_mask_real_product_segments() {
    // 正例：占位段被放过
    assert!(
        product_segments_in("\"com.bedcode.xxx\"").is_empty(),
        "格式校验夹具用的裸占位段不应被判成产品"
    );
    assert!(
        product_segments_in("\"com.bedcode.test\"").is_empty(),
        "`test` 是中性占位，不该被判成产品"
    );
    // 反例：真实产品段必须被认出（逐一列举，防有人把真产品名塞进占位表）
    for real in [
        "terminal-session",
        "agent-hub",
        "file-transfer",
        "ai-chatbox",
        "auto-task",
        "session",
    ] {
        let hit = product_segments_in(&format!("\"com.bedcode.{real}\""));
        assert!(
            hit.contains(real),
            "真实产品段 `{real}` 必须被认出（不得混入 PLACEHOLDER_SEGMENTS），实得 {hit:?}"
        );
    }
}

/// C-3｜覆盖面完整：登记 crate 真实存在，且 `packages/` 下每个 `bedcode-*` 目录
/// 都在「已扫描 ∪ 待扫描」两个桶之一里
///
/// 前半段防「登记了但不存在的 crate」；后半段防更隐蔽的「从登记表里删掉 crate」——
/// 那是一条零成本后门，锁照样绿而覆盖面少一块。两条合起来登记表无法静默缩水。
#[test]
fn every_registered_crate_is_present_and_scan_coverage_is_complete() {
    assert!(
        !SCANNED_CRATES.is_empty(),
        "登记表为空 ⇒ 本锁无管辖面，必须立即失效报错而非静默通过"
    );
    for crate_name in SCANNED_CRATES {
        let src = crate_dir(crate_name).join("src");
        assert!(
            src.is_dir(),
            "登记表列了 `{crate_name}` 但 {} 不存在 —— crate 改名/移出必须同改登记表",
            src.display()
        );
        let mut files = Vec::new();
        collect_rs_files(&src, &mut files);
        assert!(
            !files.is_empty(),
            "`{crate_name}` 的 src 下没有 .rs —— 扫描器空转，本锁对该 crate 失效"
        );
        assert!(
            !PENDING_SCAN_CRATES.iter().any(|(n, _)| n == crate_name),
            "`{crate_name}` 同时出现在 SCANNED_CRATES 与 PENDING_SCAN_CRATES —— \
             处置已完成请删掉待扫描条目"
        );
    }

    // 反向：两个 packages/ 下每个 bedcode-* 目录都必须有归属
    let mut unregistered: Vec<String> = Vec::new();
    for root in packages_dirs() {
        let entries =
            fs::read_dir(&root).unwrap_or_else(|e| panic!("读不到 {} —— 扫描器空转，本锁失效：{e}", root.display()));
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("bedcode-") {
                continue;
            }
            if SCANNED_CRATES.contains(&name.as_str()) || PENDING_SCAN_CRATES.iter().any(|(n, _)| *n == name) {
                continue;
            }
            unregistered.push(format!("{}/{name}", root.display()));
        }
    }
    assert!(
        unregistered.is_empty(),
        "以下 packages/bedcode-* crate 未被本锁管辖（要么登记进 SCANNED_CRATES，\
         要么进 PENDING_SCAN_CRATES 并写明在办票据）—— 静默漏登记等于锁失效：{:?}",
        unregistered
    );
}

/// C-4｜主判据：能力 crate 的生产代码不得含未登记的产品插件 id
#[test]
fn capability_crates_carry_no_unregistered_product_plugin_ids() {
    let mut violations: Vec<String> = Vec::new();
    let mut exemptions: Vec<(&str, &str)> = Vec::new();
    for exc in PRODUCT_ID_EXCEPTIONS {
        exemptions.push((exc.crate_name, exc.file_rel));
    }

    for crate_name in SCANNED_CRATES {
        for src in scan_crate(crate_name) {
            let segs = product_segments_in(&src.lines.join("\n"));
            if segs.is_empty() {
                continue;
            }
            if exemptions.contains(&(crate_name, src.rel.as_str())) {
                continue; // 走 C-5 的内容钉死
            }
            violations.push(format!(
                "{crate_name}/{}: 产品插件 id {:?} —— 能力 crate 不得知道「有哪些产品、\
                 各做什么」。移出本 crate（登记到宿主壳 / 做成注入端口），或在本文件模块头\
                 写明 AGENTS §5.1 的归类理由并登记到 PRODUCT_ID_EXCEPTIONS",
                src.rel, segs
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "能力 crate 生产代码含未登记的产品插件 id：\n  - {}",
        violations.join("\n  - ")
    );
}

/// C-5｜登记例外按**内容**钉死：多一条 / 少一条 / 改一条接管关系都测红
#[test]
fn registered_product_id_exceptions_are_pinned_by_content() {
    for exc in PRODUCT_ID_EXCEPTIONS {
        assert!(
            SCANNED_CRATES.contains(&exc.crate_name),
            "例外登记的 crate `{}` 不在 SCANNED_CRATES 内 —— 例外指向了不受本锁管辖的 crate",
            exc.crate_name
        );
        let srcs = scan_crate(exc.crate_name);
        let observed = observed_literals(&srcs, exc.file_rel);
        let expected: BTreeSet<String> = exc.expected_ids.iter().map(|s| s.to_string()).collect();

        assert!(
            !observed.is_empty(),
            "例外登记的文件 `{}/{}` 扫不到任何产品 id —— 文件改名/移走会让例外静默失效，\
             须同改 PRODUCT_ID_EXCEPTIONS。理由：{}",
            exc.crate_name,
            exc.file_rel,
            exc.reason
        );
        assert_eq!(
            observed, expected,
            "`{}/{}` 的产品 id 字面量与登记内容不一致（新增/删除/改接管关系都算）。\
             改动必须回本锁说明理由。归类理由：{}",
            exc.crate_name, exc.file_rel, exc.reason
        );
    }
}

/// C-6｜退役宿主面不得回接：AGENTS §5.3 已整体退役的词汇不进能力 crate 生产代码
#[test]
fn retired_host_surfaces_are_not_reintroduced_in_capability_crates() {
    let mut hits: Vec<String> = Vec::new();
    for crate_name in SCANNED_CRATES {
        for src in scan_crate(crate_name) {
            let text = src.lines.join("\n");
            for token in RETIRED_HOST_SURFACE_TOKENS {
                if text.contains(token) {
                    hits.push(format!("{crate_name}/{}: 退役面 `{token}`", src.rel));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "退役宿主面词汇回接（AGENTS §5.3 已连同权限位整体退役，业务归 \
         com.bedcode.terminal-session）：\n  - {}",
        hits.join("\n  - ")
    );
}
