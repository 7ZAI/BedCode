//! L2「内部统一业务应用」反向依赖红线（ADR 0032）+ 防回接锁
//!
//! L1 / L3 都是「宿主提供 / 被调用」方向（组件提供能力、应用调宿主）；
//! **只有 L2 是宿主内核主动调 wasm 组件**——这是本仓唯一一处宿主反向依赖业务组件。
//! 无刹车机制时它会自然蔓延成「宿主依赖一堆业务 wasm」，直接违反 AGENTS §5.1
//! 无业务内核红线。故把三条约束落成可跑的锁，而不是只写文档：
//!
//! | 约束 | 本文件如何锁 | 谁落地 |
//! | --- | --- | --- |
//! | ① 白名单式登记，不自动发现 | 锁「调用点白名单」+ 锁「桥接面导出面固定」 | 注册表本体见 ADR 0031 |
//! | ② 只做安全闸门用途 | 锁「裁决面只回裁决」：成功态是字段集**钉死**的连接身份 | 同上 |
//! | ③ 宿主只转发不解释 | 零解析转发层形状 + 签名断言 | `utils/session_gateway.rs` |
//!
//! 另有两条锁：
//!
//! - ④ `host_switches_on_role_predicates_not_role_values` 防「按角色身份分支」：
//!   宿主若开始**按角色身份**做业务判断，每加一个角色就要改宿主，红线随之从
//!   「分类学」退化成「业务识别」。
//! - ⑤ `host_has_no_entry_token_crypto` 防**宿主把入场密码学接回来**：ADR 0033
//!   删掉 `utils/auth/jwt.rs` 之后，生产路径上重新出现 `JwtService` /
//!   `verify_token_with_expiry` / `generate_device_token` 等字样，就是把
//!   「验签执行点留宿主」那条已退役口径复活了。
//!
//! ## 各锁的判据与**已知边界**（评审 2026-09-29 收紧后）
//!
//! 文本扫描类锁的固有边界，写在这里以免被当成「完备证明」：
//!
//! - 锁 1 的 needle 判据能被**重导出改名**绕过（给父模块加
//!   `pub use auth_center as gate;`，别处再 `use …::gate`）。两道措施：锁 1b 把桥接模块的
//!   **导出面钉死**（函数 / 常量 / `pub use` 别名）且**禁父模块 `pub use` 转发**——
//!   任何新增导出项或转发都会转红。仍未覆盖的是「在**白名单文件内部**建私有别名」
//!   （`use … as gate;`）与自建薄壳——那类改动必然在 review 里显形，靠 §5.1 提交前自检兜。
//! - 锁 4 挡的是「按角色身份分支」：角色**字面量**与角色 **wire 拼写串**都禁。
//!   仍未覆盖的是纯别名式绕过（`use PluginKind as PK`）——同样靠 review。
//!
//! 新增调用点时改 `L2_CONSUMER_ALLOWLIST` 并在 ADR 记理由；桥接模块增删导出项时改
//! `BRIDGE_PUBLIC_SURFACE`。两者都是**显式登记表**，绕过它们直接写代码会让本文件转红，
//! 这是刻意的：改动必须落在人可见的 diff 里。

use super::scaffold::*;
use super::*;

/// 允许触碰 L2 桥接面（`utils::auth::auth_center`）的宿主模块 —— 约束①白名单
///
/// 逐条理由：
/// - `utils/auth/auth_center.rs`：L2 桥接门本体（策略裁决 + 配对/QR/trust 零解析转发）；
/// - `../packages/bedcode-server-http/src/middleware/auth_gateway.rs`（票 04 前是
///   `src/server/http/middleware/auth_gateway.rs`）/
///   `../packages/bedcode-server-websocket/src/channel/plugin.rs`（票 05 前是
///   `src/server/websocket/channel/plugin.rs`）：**安全闸门**——HTTP 网关与 WS 通道的
///   认证中间件（唯一正当的裁决消费方）；两个面抽 crate 后它们都经 `base::ports` 的
///   `AuthCenter` 端口字段取裁决，**扫描面必须跟着代码走**（见 `L2_SCAN_ROOTS`）：
///   只扫宿主 `src` 会让这些白名单悬空、并把 crate 里新长出的越界消费点静默放行；
/// - `wasm_core/host_api/auth.rs`：**组合式认证原语**（ADR 0031 K1/K6）——权限门 +
///   唯一性仲裁的调用方 + 零解析窄转发，属安全闸门；（注册表本体
///   `wasm_core/host_api/auth_center.rs` 不在本表：它是 L2 侧的**注册表**，
///   不消费桥接面，不需要被桥接面白名单授权）
/// - `wasm_core/manager/runtime/component.rs`：ABI v32 的 `host-auth` 新导出接线
///   （权限门与派发，实现体在 `host_api/auth.rs`）；
/// - `wasm_core/manager/host/boot.rs` / `activation.rs`：生命周期闸门接线——
///   停用时 `purge_for_plugin` 回收中心句柄、启动期按在册中心做对账
///   （同 pty / ws / http / task 停用回收一组）。
/// - `utils/auth/test_tokens.rs`（**仅 `#[cfg(test)]`**）：ADR 0033 后宿主没有签发面，
///   测试也不能自己造「合法 token」——唯一诚实的造法是走生产 `auth-grant` /
///   `jwt` / `issue`。它只经 `invoke_auth_method` 零解析转发，**零解释**（约束③），
///   产出只给测试断言用（不产产品事实，约束②不适用），且不参与生产构建。
///
/// **纯模块声明文件不在表内**（`utils/auth.rs` / `host_api.rs` 的 `mod auth_center;`）：
/// 扫描器对 `mod X;` / `pub mod X;` 声明行直接跳过——声明不是「消费」。
///
/// **wasm-core 纯净性收口（票 05/05b，2026-10-06）路径随迁**：
/// - `src/utils/auth/auth_center.rs` / `src/utils/session_gateway.rs` 已回宿主 lib
///   （`../../src-tauri/src/utils/`）——白名单条目随真源迁移，不再登记 wasm-core 内
///   已删路径（悬空条目会被反向自检抓红）；
/// - `src/test_support.rs`：票 05b 上提的常编译测试基建（非 `tests/` 目录、非
///   `_test.rs`，扫描器按生产代码对待）——其 `pub use crate::host_api::auth_center::…`
///   re-export 命中 `auth_center` needle。消费的是**注册表机制面**（test_support 即
///   协调层，非 L2 业务读取），登记并注释理由。
const L2_CONSUMER_ALLOWLIST: &[&str] = &[
    "../../bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs",
    "../../bedcode-desktop/src-tauri/src/utils/session_gateway.rs",
    "src/test_support.rs",
    "src/utils/auth/test_tokens.rs",
    "../bedcode-server-http/src/middleware/auth_gateway.rs",
    "../bedcode-server-websocket/src/channel/plugin.rs",
    "../../bedcode-desktop/src-tauri/src/server/ports_impl.rs",
    "src/host_api/auth.rs",
    "src/manager/host/boot.rs",
    "src/manager/host/activation.rs",
    "src/manager/runtime/component.rs",
];

/// L2 桥接门的导出面（函数 / 常量 / `pub use` 别名）—— 锁 1b 的登记表
///
/// 加删导出项必须同改本表：新增项意味着新的宿主→L2 通路（或新的别名绕过面），
/// 需要显式裁决而不是悄悄长出来。历史：ADR 0031 组合式认证加 `invoke_auth_method`
/// 时登记过；**ADR 0033** 删两项——`call_api`（通用 JSON-RPC 客户端，上提到
/// `wasm_core::intercall`：它服务会话面也服务认证面，住在认证域是归属错位）与
/// `format_device_display_name`（死代码，HTTP 同构实现早已下沉到插件 `auth_http`）。
///
/// **票 05/05b（2026-10-06）随真源回 lib**：`enforce_connection_policy` /
/// `session_active` / `SESSION_PLUGIN_ID` 回宿主 lib（`src-tauri/src/utils/auth/`，
/// 本锁读 lib 真源钉导出面）；`invoke_auth_method`（WIT 原语实现链）并入
/// wasm-core `host_api/auth_center.rs`——它是机制面（host-auth 原语），导出变化
/// 由 lib 调用方编译期依赖天然锁定，不再需要本锁钉它。
const BRIDGE_PUBLIC_SURFACE: &[&str] = &[
    "SESSION_PLUGIN_ID",
    "enforce_connection_policy",
    "session_active",
];

/// 宿主源码的「纯代码」视图：剥掉行注释与块注释，跳过 `mod X;` 声明行
///
/// 文本扫描锁的公共前处理：注释里的 `auth_center` 不是消费点（按整行匹配会假红），
/// `mod auth_center;` 是声明也不是消费（否则每个父模块都得进白名单，白名单就失去
/// 「登记 = 显式裁决」的意义）。
fn code_lines(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block_comment = false;
    for raw in content.lines() {
        let mut line = raw.to_string();
        // ① 处于块注释内：先找 `*/` 出来，找不到就整行跳过
        if in_block_comment {
            match line.find("*/") {
                Some(end) => {
                    line = line[end + 2..].to_string();
                    in_block_comment = false;
                }
                None => continue,
            }
        }
        // ② 整行注释（含 `//!` / `///`）先丢——**必须早于块注释扫描**：文档里
        //    大量出现 `/api/auth/*`、`contributes.httpEndpoints` 这类带 `/*` 的路径
        //    形参，当成块注释开头会把后面整份文件的真实代码吃掉（假绿）
        if line.trim_start().starts_with("//") {
            continue;
        }
        // ③ 行注释：` //`（带前导空格）才算注释起点，避免砍掉 URL 里的 `://`
        let mut code = match line.find(" //") {
            Some(idx) => line[..idx].to_string(),
            None => line,
        };
        // ④ 块注释（已在 ② 排除了行注释里的假 `/*`）
        loop {
            let Some(start) = code.find("/*") else { break };
            match code[start + 2..].find("*/") {
                Some(rel_end) => {
                    code = format!("{}{}", &code[..start], &code[start + 2 + rel_end + 2..]);
                }
                None => {
                    code = code[..start].to_string();
                    in_block_comment = true;
                    break;
                }
            }
        }
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }
        // 模块声明行（`mod X;` / `pub mod X;` / `pub(crate) mod X;`）不算消费
        let decl_prefix = trimmed
            .strip_prefix("pub(crate) ")
            .or_else(|| trimmed.strip_prefix("pub "))
            .unwrap_or(trimmed);
        if decl_prefix.starts_with("mod ") && decl_prefix.trim_end().ends_with(';') {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

/// 扫描器自身的用例（它四条锁的地基，错一步就是假绿）
#[test]
fn code_lines_strips_comments_without_swallowing_code() {
    let src = r#"
//! - /api/auth/* — 放行（文档里的 `/*` 形参，不得当成块注释开头）
/// 文档行：mentions auth_center
/* 块注释里的 auth_center */
pub mod auth_center;                 // 模块声明不算消费
use crate::utils::auth::auth_center; // 真实消费
let url = "https://x/y";             // `://` 不是行注释
let n = 1; /* 行内块注释 auth_center */ let m = 2;
"#;
    let lines = code_lines(src);
    assert!(
        !lines.iter().any(|l| l.contains("放行") || l.contains("mentions")),
        "文档行必须被剥掉: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("mod auth_center")),
        "模块声明不算消费: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("use crate::utils::auth::auth_center")),
        "真实消费必须保留: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("https://x/y")),
        "URL 不得被行注释截断: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("行内块注释")),
        "行内块注释必须被剥掉: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("let m = 2")),
        "行内块注释之后的代码必须保留: {lines:?}"
    );
}

/// 只扫**生产代码**：跳过文件内联的 `#[cfg(test)]` 段
///
/// 内联 `#[cfg(test)]` 里出现角色 wire 串、角色值、乃至桥接面导出都是**测试职责**
/// （取值域收口断言、每角色就绪判定），不是生产分支；不跳过会把它们误判成越界
/// （假红），逼着人往白名单里塞测试文件（白名单随即失去意义）。
///
/// 实现要点：**按花括号配对**跳过整个 `#[cfg(test)] mod tests { … }` / 具名
/// `#[cfg(test)] fn … { … }`，而不是「截断到第一个 `#[cfg(test)]`」——后者在本仓
/// 真实文件上就会假绿（`host_api/auth.rs` 第 13 行有一个 `#[cfg(test)]` 辅助项，
/// 截断会把其后全部生产函数一起吃掉）。
///
/// 已知边界：字符串字面量里的花括号会计入配对（极少见，且只会把某段测试段的范围
/// 算错 → 假红/漏扫各半）；靠 review 兜。
fn production_code_lines(content: &str) -> Vec<String> {
    let lines = code_lines(content);
    let mut out = Vec::new();
    let mut pending_cfg_test = false;
    // Some(剩余闭合深度)：处于 `#[cfg(test)]` 段内
    let mut test_depth: Option<i32> = None;
    for line in lines {
        let braces = |s: &str| -> i32 { s.matches('{').count() as i32 - s.matches('}').count() as i32 };
        if let Some(remaining) = test_depth.as_mut() {
            *remaining += braces(&line);
            if *remaining <= 0 {
                test_depth = None;
            }
            continue;
        }
        if pending_cfg_test {
            pending_cfg_test = false;
            if line.contains('{') {
                let depth = braces(&line);
                if depth > 0 {
                    test_depth = Some(depth);
                    continue;
                }
            }
            // 无花括号的 `#[cfg(test)]` 项（如 `static X: Mutex<()> = …;`）= 单行声明，
            // 不构成「段」：不跳过后续行
            continue;
        }
        if line == "#[cfg(test)]" {
            pending_cfg_test = true;
            continue;
        }
        if line.contains("#[cfg(test)]") && line.contains('{') {
            // `#[cfg(test)] mod tests {` 同行写法
            let depth = braces(&line);
            if depth > 0 {
                test_depth = Some(depth);
            }
            continue;
        }
        out.push(line);
    }
    out
}

/// `production_code_lines` 的用例：测试段被跳过、其后的生产代码不被误伤
#[test]
fn production_code_lines_skips_only_the_test_region() {
    let src = r#"
#[cfg(test)]
fn test_helper() {
    let x = "internal-business";
}
pub(crate) fn auth_center_register() {
    crate::host_api::auth_center::register("x");
}
#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        assert_eq!("business-app", "business-app");
    }
}
pub fn after_tests() {
    use crate::utils::auth::auth_center::session_active;
}
"#;
    let lines = production_code_lines(src);
    assert!(
        lines.iter().any(|l| l.contains("auth_center_register")),
        "测试辅助项之后的**生产代码**必须保留: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("after_tests")),
        "测试段之后的**生产代码**必须保留: {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("internal-business") || l.contains("business-app")),
        "测试段内容必须被跳过: {lines:?}"
    );
}

/// L2 消费点的扫描根（相对 `src-tauri`）
///
/// server-lib-split 票 04：HTTP 面抽成 `bedcode-server-http` crate 后，闸门消费点
/// （`middleware/auth_gateway.rs`）住在 crate 里——扫描面不跟着代码走，等于让那条
/// 白名单悬空、并把 crate 内新长出的越界消费点静默放行。票 07 补齐其余三个消费侧
/// crate（core / peer-net / crypto-engine）：它们同样能经 `bedcode_server_base::ports`
/// 取 `AuthCenter` 端口，扫描面漏掉就等于给它们开了免检通道。
///
/// 只纳**消费侧** crate：`bedcode-server-base::ports` 是 `AuthCenter` 端口与
/// `ServerPorts.auth_center` 字段的**定义方**，把它扫进来等于给词汇定义开白名单，
/// 本锁「登记 = 显式裁决」的语义就没了。
const L2_SCAN_ROOTS: &[&str] = &[
    "src",
    // 整核抽出 + 2026-10-08 迁根：宿主壳（ports_impl 等 L2 消费点）留在 lib
    // `bedcode-desktop/src-tauri/src`，扫描面必须跟（相对本 crate 根回两级再进桌面）
    "../../bedcode-desktop/src-tauri/src",
    "../bedcode-server-core/src",
    "../bedcode-server-http/src",
    "../bedcode-server-websocket/src",
    "../bedcode-server-peer-net/src",
    "../bedcode-crypto-engine/src",
];

/// 收集扫描根下「纯代码」命中 needle 的非测试 .rs 文件（整核抽出后相对本 crate 根）
fn host_files_mentioning(needle: &str) -> Vec<String> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = L2_SCAN_ROOTS.iter().map(|root| base.join(root)).collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // 锁自身与用例目录携带这些标识符（以字符串形式），不参与扫描
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with("_test.rs") || rel.contains("/tests/") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if production_code_lines(&content).iter().any(|line| line.contains(needle)) {
                hits.push(rel);
            }
        }
    }
    hits.sort();
    hits
}

/// 防回接锁：宿主对 L2 只有「裁决调用」与「零解析转发」，无业务读取
///
/// 断言宿主源码里提到 L2 桥接面的文件集合 ⊆ 白名单。任何新增消费点都必须显式
/// 改本文件（改动的意图随 diff 可见），而不是悄悄长出一条「宿主依赖业务 wasm」的
/// 通路。**方向性**：本锁不禁止「宿主调插件」——那正是 L1 能力路由在做的事；
/// 它禁止的是**在白名单之外**出现新的 L2 消费点。
#[test]
fn internal_business_host_dependency_stays_gated() {
    let hits = host_files_mentioning("auth_center");
    let unexpected: Vec<&String> = hits
        .iter()
        .filter(|rel| !L2_CONSUMER_ALLOWLIST.contains(&rel.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "宿主出现白名单外的 L2（内部统一业务应用）消费点：\n{}\n\
         ——L2 是宿主唯一的反向依赖类别，新增消费点必须先过 ADR 0031 三条红线 \
         （白名单式登记 / 只做安全闸门 / 只转发不解释），并把文件登记进 \
         L2_CONSUMER_ALLOWLIST",
        unexpected.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")
    );
    // 反向自检：白名单条目不得悬空（重命名/删除模块后本锁要能看出来，而不是
    // 因为「命中面变空」而假绿）
    for allowed in L2_CONSUMER_ALLOWLIST {
        assert!(
            hits.iter().any(|rel| rel == allowed),
            "白名单条目 {allowed} 已不再引用 L2 桥接面——请核实后删除（否则白名单腐化，\
             新的越界消费点会被它静默放行）"
        );
    }
}

/// 防回接锁 1b：L2 桥接面的**导出面固定** + **父模块不得转发**（堵「重导出改名」绕过）
///
/// 锁 1 的 needle 判据能被 `pub use auth_center as gate;` 这类别名绕过（别名里没有
/// `auth_center` 字面量，别的文件再 `use …::gate` 就完全躲开扫描）。两道措施：
/// ① 桥接模块自身的导出项（函数 / 常量 / `pub use` 别名）钉死成
///    `BRIDGE_PUBLIC_SURFACE`——任何新增导出项（含别名）都转红；
/// ② 父模块（`utils/auth.rs` / `host_api.rs`）**不得** `pub use` 桥接面——
///    普通的 `pub use jwt::*;` 不受影响，只挡「把桥接面换个名字转发出去」。
///
/// 仍未覆盖的边界写在文件头「已知边界」：白名单文件内部的**私有**别名
/// （`use … as gate;`）与自建薄壳，靠 review + §5.1 提交前自检兜。
#[test]
fn l2_bridge_public_surface_is_pinned() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // 2026-10-08 迁根：本 crate 在根 packages/，auth_center 真源在 bedcode-desktop/src-tauri/
    let path = base.join("../../bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs");
    let content = std::fs::read_to_string(&path).expect("read L2 bridge module");
    let mut actual: Vec<String> = Vec::new();
    for line in production_code_lines(&content) {
        // `pub use a::b as c;` → 记别名 c（未 as 时记末段 b）
        if let Some(rest) = line.strip_prefix("pub use ") {
            let item = rest.trim_end_matches(';').trim();
            let name = item.rsplit(" as ").next().unwrap_or(item).trim();
            actual.push(name.rsplit("::").next().unwrap_or(name).to_string());
            continue;
        }
        // 只认**以 `pub` 开头的声明行**（私有项不是导出面）
        let decl = match line.strip_prefix("pub(crate) ").or_else(|| line.strip_prefix("pub ")) {
            Some(rest) => rest,
            None => continue,
        };
        if let Some(rest) = decl.strip_prefix("fn ") {
            actual.push(rest.split('(').next().unwrap_or(rest).trim().to_string());
        } else if let Some(rest) = decl.strip_prefix("const ") {
            actual.push(rest.split(':').next().unwrap_or(rest).trim().to_string());
        }
    }
    actual.sort();
    let mut expected: Vec<String> = BRIDGE_PUBLIC_SURFACE.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "L2 桥接门的导出面变了：新增/删除导出项（含 `pub use … as …` 别名）会开出新的\
         宿主→L2 通路或新的别名绕过面，必须先裁决并同改 BRIDGE_PUBLIC_SURFACE"
    );

    // 父模块不得把桥接面转发出去（`pub use auth_center …` / `pub use auth_center::…`）
    for parent in ["src/utils/auth.rs", "src/host_api.rs"] {
        let content = std::fs::read_to_string(base.join(parent)).expect("read parent module");
        let forwards: Vec<String> = production_code_lines(&content)
            .into_iter()
            .filter(|line| line.starts_with("pub use ") && line.contains("auth_center"))
            .collect();
        assert!(
            forwards.is_empty(),
            "{parent} 把 L2 桥接面 `pub use` 转发出去了（别名绕过面）：{forwards:?}——\
             需要新通路时直接调用 `utils::auth::auth_center::…`，不要换名转发"
        );
    }
}

/// 从 `pub fn enforce_connection_policy` 声明行起收集到 `{` 之前的签名（跨行安全）
///
/// 签名被拆成多行时不得假红——按「第一条含函数名的行」判定是脆的（评审 2026-09-29）。
fn gate_signature(lines: &[&str]) -> Option<String> {
    let start = lines
        .iter()
        .position(|line| line.contains("pub fn enforce_connection_policy"))?;
    let mut signature = String::new();
    for line in &lines[start..] {
        signature.push_str(line);
        if line.contains('{') {
            break;
        }
    }
    Some(
        signature
            .split('{')
            .next()
            .unwrap_or(&signature)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// 裁决门成功态允许携带的唯一载荷：字段集**钉死**的连接身份
///
/// ADR 0033 修订了本锁的形状（v32 时成功态是 `()`）。修订**不是**放宽：
/// - 中心交回连接身份是**传输面必需**的——HTTP 中间件要注入请求上下文、
///   `caller` 转发要 `deviceId` / `deviceName`、WS 连接会话要脱敏身份、
///   日志要 `device_id`。没有它宿主只能「验签通过但不知道是谁」。
/// - 但身份**只是同一份数据的换来源**（宿主验签结果 → 中心裁决结果），不是新增
///   产品事实。所以本锁把成功态钉成**恰好三个字段**：配对记录 / 信任列表 / 设备
///   档案 / 撤销状态一律不许进宿主（那些留在中心私有库，按需走别的面取）。
const IDENTITY_PAYLOAD_FIELDS: &[&str] = &["device_id", "device_name", "fingerprint"];

/// 裁决门签名是否「只回裁决」：`Result<AuthenticatedIdentity, String>` + 无 `&mut`
/// 出参 + 参数表恰为「宿主门面 + 待裁决凭据」两项只读引用
fn gate_signature_is_decision_only(signature: &str) -> bool {
    if !signature.contains("Result<AuthenticatedIdentity, String>") || signature.contains("&mut") {
        return false;
    }
    // 参数表归一化：逐项 trim + 丢空项 + 重新拼接——否则跨行签名的尾逗号
    // （`token: &str,` + `)`）会让精确比较假红
    let normalized_params = signature
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(params, _)| {
            params
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    normalized_params == "plugin_host: &PluginHost, token: &str"
}

/// 签名判定器自身的用例（假红与两种绕过都要在这里钉住）
///
/// - 单行 / **跨行**签名都判绿（拆行不是绕过，也不是假红来源）
/// - 退回 `Result<(), String>` 判红：那是 v32 的形状，v33 起中心必须交回身份
/// - 返回**别的**载荷（如 `Vec<String>`）判红：宿主能解析产品事实
/// - 加 `&mut` 出参判红：门内攒事实
/// - 加工具参数判红：宿主开始攒上下文
#[test]
fn gate_signature_judgement_handles_wrapping_and_rejects_payloads() {
    for ok in [
        "pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str) -> std::result::Result<AuthenticatedIdentity, String> {",
        "pub fn enforce_connection_policy(\n    plugin_host: &PluginHost,\n    token: &str,\n) -> std::result::Result<AuthenticatedIdentity, String> {\n    let _ = 1;",
    ] {
        let lines: Vec<&str> = ok.lines().collect();
        let sig = gate_signature(&lines).expect("signature found");
        assert!(gate_signature_is_decision_only(&sig), "必须判绿: {sig}");
    }
    for bad in [
        // v32 形状（`()`）不再允许：中心放行时必须给出连接身份
        "pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str) -> std::result::Result<(), String> {",
        // 产品事实载荷
        "pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str) -> std::result::Result<Vec<String>, String> {",
        "pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str, log: &mut Vec<String>) -> std::result::Result<AuthenticatedIdentity, String> {",
        "pub fn enforce_connection_policy(plugin_host: &PluginHost, token: &str, device: &str) -> std::result::Result<AuthenticatedIdentity, String> {",
    ] {
        let lines: Vec<&str> = bad.lines().collect();
        let sig = gate_signature(&lines).expect("signature found");
        assert!(!gate_signature_is_decision_only(&sig), "必须判红: {sig}");
    }
}

/// 防回接锁：L2 裁决面只回裁决，宿主不解析其返回值（约束②③）
///
/// `enforce_connection_policy` 的成功态是一枚**字段集钉死**的连接身份、失败态是
/// 原因串——宿主据此决定放行 / 拒绝，**不**从中读任何产品事实。签名一旦变成别的
/// 载荷（配对记录 / 信任列表 / 设备档案），或长出 `&mut` 出参 / 额外上下文参数，
/// 就是「宿主开始解释 L2 的业务语义」，本条立即转红。判据见
/// [`gate_signature_is_decision_only`]。
#[test]
fn l2_gate_returns_decision_only() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-desktop/src-tauri/src/utils/auth/auth_center.rs");
    let content = std::fs::read_to_string(&path).expect("read L2 bridge module");
    let lines: Vec<&str> = content.lines().collect();
    let signature = gate_signature(&lines).expect("L2 裁决门必须存在于 utils/auth/auth_center.rs");
    assert!(
        gate_signature_is_decision_only(&signature),
        "L2 裁决门成功态只允许是连接身份、且无出参 / 无额外上下文：{signature}"
    );
}

/// 防回接锁（ADR 0033 修订收紧）：连接身份载荷的字段集必须**恰好**是三个
///
/// 这是本文件对「宿主从认证中心返回值里读产品事实」这条红线的**具体封口**。
/// 中心加字段（配对 id、信任等级、撤销状态……）时这个类型不得跟着长——那些事实
/// 留在中心私有库，宿主要就另开一条按需取的面并各自裁决。反过来，删字段也转红
/// （那说明传输面必需的身份信息被弄丢了）。
#[test]
fn l2_identity_payload_is_pinned_identity_only() {
    // server-lib-split：AuthenticatedIdentity 真源在 bedcode-server-base crate
    // （packages/bedcode-server-base/src/identity.rs；utils/auth/identity.rs 为
    // re-export 薄壳），锁跨 crate 读真源文件
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bedcode-server-base/src/identity.rs");
    let content = std::fs::read_to_string(&path).expect("read identity module");
    let body = content
        .split("pub struct AuthenticatedIdentity {")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .expect("AuthenticatedIdentity struct body");
    let mut actual: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        // 只认字段声明行（`pub <name>: <type>`），跳过属性行与注释
        let Some(rest) = trimmed.strip_prefix("pub ") else {
            continue;
        };
        let Some(name) = rest.split(':').next() else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        actual.push(name.to_string());
    }
    let mut expected: Vec<String> = IDENTITY_PAYLOAD_FIELDS.iter().map(|s| s.to_string()).collect();
    expected.sort();
    actual.sort();
    assert_eq!(
        actual, expected,
        "连接身份字段集变了：宿主只允许持有 {expected:?}（传输面必需），\
         产品事实一律留在中心私有库（ADR 0033 修订 L2 锁）"
    );
}

/// 防回接锁（ADR 0033 fail-visible ③）：宿主生产路径不得再出现入场密码学
///
/// 判据 = 文本扫描 + 与测试/注释区隔离：ADR 0033 删掉了 `utils/auth/jwt.rs`
/// （`JwtService` / `generate_device_token` / `verify_device_token` /
/// `verify_token_with_expiry`）与两个 `host-auth` 原语。任何一处重新出现，
/// 都意味着「验签执行点留宿主」那条**已退役**的口径被复活——而那正是本专项
/// 要消除的错位二（注释说「密钥不出宿主」，代码里插件自持明文副本）。
///
/// 边界：测试代码里出现这些字样是允许的（回归锁自身 + 探针文档），扫描只取
/// 「纯代码」视图（剥注释）且排除 `#[cfg(test)]` 模块。
#[test]
fn host_has_no_entry_token_crypto() {
    const NEEDLES: &[&str] = &[
        "JwtService",
        "verify_token_with_expiry",
        "generate_device_token",
        "verify_device_token",
        "device_token_issue",
        "device_token_verify",
        "JwtClaims",
    ];
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for rel in host_files_mentioning("utils::auth") {
        let Ok(content) = std::fs::read_to_string(base.join(&rel)) else {
            continue;
        };
        for line in code_lines(&content) {
            if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
                break;
            }
            for needle in NEEDLES {
                if line.contains(needle) {
                    violations.push(format!("{rel}: {needle} → {}", line.trim()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "宿主生产路径不得再持有设备入场密码学（ADR 0033）：{violations:#?}"
    );
}

/// 防回接锁（B-downsink fail-visible ③）：宿主生产路径不得再出现生物凭证密码学
///
/// v34（2026-09-30）把生物凭证公钥托管 + P-256 验签执行从宿主下沉认证中心
/// （`auth_biometric_keys` 私有库 + WASM 内 p256）；宿主 `utils/auth/biometric.rs`
/// 整模块删除。判据同 v33 入场密码学锁：`host-auth` 三个生物原语 / 宿主验签函数 /
/// 挑战管理器任何一处重新出现在**生产**代码里，都意味着「公钥托管 + 验签留宿主」
/// 那条已退役的口径被复活——本专项要消除的正是它。
///
/// 边界：测试代码里出现这些字样是允许的（回归锁自身 + 集成测试的名称/白盒查询），
/// 扫描只取「纯代码」视图（剥注释）且排除 `#[cfg(test)]` 模块与 `tests/` 目录。
#[test]
fn host_has_no_biometric_crypto() {
    const NEEDLES: &[&str] = &[
        "auth_biometric_credential_bound",
        "auth_biometric_verify_signature",
        "auth_biometric_credential_bind",
        "verify_biometric_signature",
        "BiometricChallengeManager",
        "biometric_secret_key",
    ];
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for rel in host_files_mentioning("utils::auth") {
        let Ok(content) = std::fs::read_to_string(base.join(&rel)) else {
            continue;
        };
        for line in production_code_lines(&content) {
            for needle in NEEDLES {
                if line.contains(needle) {
                    violations.push(format!("{rel}: {needle} → {}", line.trim()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "宿主生产路径不得再持有生物凭证密码学（B-downsink）：{violations:#?}"
    );
}

/// 防回接锁：宿主只按**谓词**判断角色，不认识具体角色值
///
/// `PluginKind::BasicService` / `InternalBusiness` / `BusinessApp` 三个字面量
/// 不得出现在宿主源码：宿主只用 `is_role_driven()` / `provides_host_capabilities()` /
/// `is_business_app()` 三个谓词，加载顺序取自 SDK 常量
/// `PluginKind::ROLE_DRIVEN_LOAD_ORDER`。这样新增角色不必改宿主，分类学也不会
/// 退化成「宿主按角色名做业务分支」（那正是红线要防的蔓延形态）。
///
/// 同时禁**角色 wire 拼写串**（`"basic-service"` 等）——它们是 `kind.as_str()`
/// 比较与手写字符串分支的入口（`as_str` / `label()` 正是本批为暴露值而加的）。
/// 仍未覆盖的别名式绕过（`use PluginKind as PK`）在本文件头「已知边界」里登记，
/// 靠 review + §5.1 提交前自检兜底。
#[test]
fn host_switches_on_role_predicates_not_role_values() {
    let mut violations = Vec::new();
    for value in ["BasicService", "InternalBusiness", "BusinessApp"] {
        for rel in host_files_mentioning(&format!("PluginKind::{value}")) {
            if !ROLE_VALUE_ALLOWLIST.contains(&rel.as_str()) {
                violations.push(format!("{rel}: PluginKind::{value}"));
            }
        }
    }
    for wire in ["\"basic-service\"", "\"internal-business\"", "\"business-app\""] {
        for rel in host_files_mentioning(wire) {
            if !ROLE_VALUE_ALLOWLIST.contains(&rel.as_str()) {
                violations.push(format!("{rel}: 角色 wire 串 {wire}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "宿主出现按角色身份分支的代码（应改用 PluginKind 谓词 / ROLE_DRIVEN_LOAD_ORDER，\
         或把自己登记进 ROLE_VALUE_ALLOWLIST 并在 ADR 记理由）：\n{}",
        violations.join("\n")
    );
}

/// 允许按**角色身份**判定的宿主文件（当前为空）
///
/// 空 = 宿主对三个角色一视同仁，只用谓词；每次往里加一行都是一次显式决策。
/// （`validation.rs` 的取值域断言在内联测试段内，扫描已按 `production_code_lines`
/// 截断，故**不需要**登记——这正是白名单必须保持干净的原因。）
const ROLE_VALUE_ALLOWLIST: &[&str] = &[];
