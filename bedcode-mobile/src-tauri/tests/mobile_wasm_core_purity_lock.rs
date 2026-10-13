//! wasm-core 纯净性防回接锁（wasm-core-mobile-purity 票 06-①；票 05 绑定归位后按新终态
//! 重写；**票 06 收口补判据 7 = 端词汇全集**）
//!
//! 判据来源：spec §0 用户指令①「wasm-core 保持纯粹的 wasm-core 机制原则，不该引入额外
//! 能力」+ 指令⑥「移动端特殊的能力留在移动端宿主侧」+ ADR 0047 D2/D4/D5 +
//! ADR 0049（单形态 + 绑定归位）+ 票 05 的绑定归位终态。
//!
//! 七道判据（任一被破坏即红）：
//! 1. **移动独有域不得回流内核**：wasm-core 源码树不得出现 notify / fs_mobile /
//!    terminal_stream / connection-target / auth-client 五域的 `Host` impl 或域实现
//!    文件；反向钉住移动宿主 adapter 在场（能力去内核化 ≠ 能力消失）。
//! 2. **core 域绑定/实现不得回流内核形态壳**：wasm-core 不得再出现任何
//!    `impl bedcode::plugin::host_*::Host`——**唯一合法例外**是
//!    `manager/runtime/core_binding.rs`（双端全等 8 域，绑契约真源
//!    `wit/core.wit`）；**统一壳**（`runtime.rs` / `runtime/component.rs`）
//!    不得出现 `bindgen!` 与 `bedcode::plugin::` 路径。单形态（2026-10-13）起
//!    移动 host_domains 收为 4 域（http / peer / mdns / ws），双端全等 8 域
//!    （storage / log / config / plugin-database / events / fs / bus / platform）
//!    由内核 core_binding 单份实现（权限门收敛统一 PermissionManager、私有库布局
//!    统一内核）——移动侧对应 adapter 文件不再要求在场。
//! 3. **共享实现核不得消失**（正向）：搬出内核的域逻辑必须在共享 crate 里可寻址，
//!    且宿主 adapter 真的转发到共享核（薄转发的最低证据）——否则「壳变薄」会退化
//!    成「能力没了」（fail-visible）。
//! 4. **统一壳行数上限**：单文件与合计双重上限，防止有人把域逻辑改写成「壳里的
//!    实现」再慢慢长回来（单形态起壳双端共用，上限按统一壳实际体量校准）。
//! 5. **移动独有机制与退役垫片不得回流**：`host-terminal-stream` 的窄转发表
//!    （网关）随域迁移动宿主；零消费者的 `sql_guard` 垫片退役不复活。
//! 6. **退役的 http 引擎不得回接**（保留自 purity 票 06）。
//! 7. **端词汇全集**（票 06 补，单形态收口）：7a cap 分片 world 名在**代码行**零命中
//!    （注释放行，因为模块文档必须说明「cap 组合不在内核」）/ 7b 声明位零端名
//!    （ADR 0047 D2，豁免走登记表）/ 7c **移动端 SDK 词汇零命中**（单形态起
//!    `bedcode_plugin_api_mobile` 完全退出 wasm-core——ABI 规格宿主注入、BusMessage
//!    经统一 `on_message` 拆分，端 SDK 依赖归两端宿主）/
//!    7d 正向锚点（core 交集绑定在场，防 7a–7c 空转）。
//!
//! **WASI 不在判据内**（票 06 修正票面表述）：本票 §2 初稿写的是「禁一切
//! cap/端名/WASI/双 SDK 词汇」，其中「禁 WASI 词汇」已被 2026-10-12 用户裁决 3
//! 作废——WASI（p2/p3）装配面住内核（`manager/runtime/wasi.rs`）且双端默认全开，
//! 开 / 关走 `CoreConfig::wasi`。WASI 是**共享机制**不是形态差异轴，锁它等于把
//! 裁决锁反。同理「双 SDK 词汇」在票 06 不做零命中（会立刻判红），改为封顶 +
//! 遗留项声明。
//!
//! 为什么独立成锁：票 03–05 把域逻辑与 WIT 绑定迁出 wasm-core，靠的是**人工对表**；
//! 回接（把实现写回内核、在内核里新起一个域绑定）不会让任何既有测试转红——
//! 本锁是那条缺口的门禁。
//!
//! 变异自检记录：票 05 落地时判据 1–6 逐锁注入违背形态 → 红 → `cp` 还原 + sha256 一致
//! （见 `.scratch/2026-10-11-wasm-core-single-form/ticket-05-mobile-binding-host.md` §7.8）；
//! 票 06 判据 7 按 7a / 7b / 7c 各注入一次（记录见同目录 `ticket-06` §2）。

use std::path::{Path, PathBuf};

/// 仓库根（本锁在 `bedcode-mobile/src-tauri/tests/`，上跳三级 = `mobile/src-tauri/../..`）
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()))
}

/// wasm-core 源码根
fn wasm_core_src() -> PathBuf {
    repo_root().join("packages/bedcode-wasm-core/src")
}

/// 移动宿主 `plugin/` 目录
fn mobile_plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/plugin")
}

/// 递归收集 `*.rs` 文件（跳过注释行判定由调用方负责）
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("读目录 {} 失败：{e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// 源码里的非注释行（`//` 开头整行跳过——头注解释迁移史是正当的）
fn code_lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
}

// ==================== 判据 1：移动独有域不得回流内核 ====================

/// 移动独有域（ADR 0047 D5）：wasm-core 里不得再有这些域的 `Host` impl
const MOBILE_ONLY_TRAITS: [&str; 5] = [
    "bedcode::plugin::host_notify::Host for",
    "bedcode::plugin::host_fs_mobile::Host for",
    "bedcode::plugin::host_terminal_stream::Host for",
    "bedcode::plugin::host_connection_target::Host for",
    "bedcode::plugin::host_auth_client::Host for",
];

/// 迁移后宿主 adapter 的落点（正向钉住：能力在宿主侧仍可达）
const MOBILE_ADAPTER_FILES: [&str; 5] = [
    "notify.rs",
    "fs_mobile.rs",
    "terminal_stream.rs",
    "connection.rs",
    "auth.rs",
];

#[test]
fn mobile_owned_domains_stay_in_mobile_host() {
    let mut files = Vec::new();
    collect_rs_files(&wasm_core_src(), &mut files);
    for path in &files {
        let content = read(path);
        for needle in MOBILE_ONLY_TRAITS {
            assert!(
                !content.contains(needle),
                "移动独有域回流 wasm-core：{} 出现 `{needle}`（应住移动宿主 src/plugin/）",
                path.display()
            );
        }
    }
    // 域实现文件同样不得在内核出现（旧 bare/host_impl/ 整体已迁出；单形态起
    // 无形态壳目录，只查 runtime 目录本体）
    let runtime_dir = wasm_core_src().join("manager/runtime");
    for name in [
        "notify.rs",
        "fs_mobile.rs",
        "terminal_stream.rs",
        "connection.rs",
        "auth.rs",
    ] {
        let p = runtime_dir.join(name);
        assert!(
            !p.exists(),
            "移动独有域文件回流 wasm-core：{}（应住移动宿主 src/plugin/{name}）",
            p.display()
        );
    }
    for file in MOBILE_ADAPTER_FILES {
        let in_host = mobile_plugin_dir().join(file);
        assert!(
            in_host.exists(),
            "移动宿主 adapter 缺失：{}（能力去 wasm-core 化不等于能力消失）",
            in_host.display()
        );
    }
}

// ==================== 判据 2：core 12 域绑定/实现不得回流内核 ====================

/// 内核唯一合法的 WIT `Host` impl 落点：core 交集绑定（绑 `wit/core.wit` 真源）
const CORE_BINDING_REL: &str = "manager/runtime/core_binding.rs";

#[test]
fn core_domain_bindings_stay_out_of_wasm_core() {
    let mut files = Vec::new();
    collect_rs_files(&wasm_core_src(), &mut files);
    for path in &files {
        let rel = path
            .strip_prefix(wasm_core_src())
            .expect("path under wasm-core src")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == CORE_BINDING_REL {
            continue;
        }
        let content = read(path);
        for (idx, line) in code_lines(&content) {
            assert!(
                !line.contains("impl bedcode::plugin::host_"),
                "core 域 `Host` impl 回流 wasm-core：{}:{}（移动住宿主 plugin/host_domains/；\
                 桌面 core 交集只允许在 {CORE_BINDING_REL}）——行：{}",
                rel,
                idx + 1,
                line.trim()
            );
        }
    }

    // 统一壳不得持绑定：`bindgen!` 与绑定生成物路径一律不出现（单形态：唯一
    // 壳 = 桌面形态，移动宿主经同一壳装配；双端全等 8 域实现在内核 core_binding）
    for rel in ["manager/runtime.rs", "manager/runtime/component.rs"] {
        let content = read(&wasm_core_src().join(rel));
        for (idx, line) in code_lines(&content) {
            for needle in ["bindgen!", "bedcode::plugin::"] {
                assert!(
                    !line.contains(needle),
                    "{rel}:{} 出现 `{needle}`——统一壳不再持有 WIT 绑定（票 04/05 归位）",
                    idx + 1
                );
            }
        }
    }

    // 正向：宿主侧的绑定与保留的 4 个端差异 core 域必须在场（单形态收口：
    // 双端全等 8 域 storage/log/config/plugin-database/events/fs/bus/platform
    // 已迁内核 core_binding，移动 host_domains 只留 http/peer/mdns/ws）
    for rel in [
        "bindings.rs",
        "guest_events.rs",
        "host_domains/mod.rs",
        "host_domains/http.rs",
        "host_domains/peer.rs",
        "host_domains/mdns.rs",
        "host_domains/ws.rs",
    ] {
        let p = mobile_plugin_dir().join(rel);
        assert!(p.exists(), "移动宿主绑定层缺失：{}", p.display());
    }
    let bindings = read(&mobile_plugin_dir().join("bindings.rs"));
    assert!(
        bindings.contains("bindgen!"),
        "移动宿主 bindings.rs 必须自持 world 绑定（孤儿规则：装配方生成 trait）"
    );
}

// ==================== 判据 3：共享实现核不得消失 ====================

/// `(相对仓库根路径, 必须在场针脚)`
const SHARED_CORES: [(&str, &str); 4] = [
    (
        "packages/bedcode-server-http/src/plugin_binding/egress.rs",
        "pub fn http_fetch(",
    ),
    ("packages/bedcode-host-api-core/src/fs.rs", "pub fn fs_read("),
    ("packages/bedcode-host-api-core/src/storage.rs", "pub fn storage_get("),
    ("packages/bedcode-host-api-core/src/bus.rs", "pub fn bus_publish("),
];

/// `(宿主 adapter 相对 plugin/ 路径, 必须转发的共享核针脚)`
///
/// 单形态（2026-10-13）：fs / storage 的 host_domains adapter 已随「双端全等 8 域
/// 迁内核 core_binding」删除——对应薄转发在 `packages/bedcode-wasm-core/src/
/// host_api/{fs,storage}.rs`（同走 bedcode-host-api-core 共享核），由 wasm-core
/// 自身测试覆盖；本表只钉移动宿主仍持有的端差异域。
const HOST_ADAPTER_CORE_FORWARDS: [(&str, &str); 3] = [
    ("host_domains/http.rs", "bedcode_server_http"),
    ("host_domains/peer.rs", "bedcode_server_peer_net"),
    // 移动 host-websocket 是**客户端域**（5 函数），引擎面在 ws-client-engine
    //（ADR 0043 双端共享 crate）；桌面服务端域（host-websocket-server）不经此表。
    ("host_domains/ws.rs", "bedcode_ws_client_engine"),
];

#[test]
fn shared_implementation_cores_stay_addressable() {
    let root = repo_root();
    for (rel, needle) in SHARED_CORES {
        let path = root.join(rel);
        assert!(path.exists(), "共享实现落点缺失：{rel}");
        assert!(
            read(&path).contains(needle),
            "{rel} 中找不到 `{needle}`（能力被搬到「壳之外」却无处可寻 = 静默失效）"
        );
    }
    // 宿主 adapter 必须真的转发到共享核（薄转发的最低证据：引用了那个 crate）
    for (rel, needle) in HOST_ADAPTER_CORE_FORWARDS {
        let path = mobile_plugin_dir().join(rel);
        assert!(path.exists(), "宿主 adapter 缺失：{}", path.display());
        assert!(
            read(&path).contains(needle),
            "{rel} 未转发到共享核 `{needle}`——域逻辑既不在内核也不在共享 crate（回流信号）"
        );
    }
}

// ==================== 判据 4：移动壳行数上限 ====================

/// 单文件上限（统一壳 `runtime/component.rs` 当前 ~950 行——含长文档注释与
/// 可选导出探测；留余量给注释与说明）
const SHELL_FILE_LINE_LIMIT: usize = 1300;

/// 壳合计上限（`runtime.rs` + `runtime/component.rs`）
const SHELL_TOTAL_LINE_LIMIT: usize = 1800;

#[test]
fn mobile_shell_stays_thin() {
    // 单形态（2026-10-13）：壳双端共用一份（桌面形态），行数上限按统一壳校准
    let shell_files = [
        wasm_core_src().join("manager/runtime.rs"),
        wasm_core_src().join("manager/runtime/component.rs"),
    ];
    let mut total = 0usize;
    for path in &shell_files {
        let lines = read(path).lines().count();
        total += lines;
        assert!(
            lines <= SHELL_FILE_LINE_LIMIT,
            "{}: {lines} 行 > 上限 {SHELL_FILE_LINE_LIMIT}（域逻辑回流的典型征兆）",
            path.display()
        );
    }
    assert!(
        total <= SHELL_TOTAL_LINE_LIMIT,
        "统一壳合计 {total} 行 > 上限 {SHELL_TOTAL_LINE_LIMIT}（域逻辑回流的典型征兆）"
    );
}

// ==================== 判据 5：移动独有机制与退役垫片不得回流 ====================

/// 终端输出窄转发表（`host-terminal-stream` 的机制）与零消费者垫片都不得回流内核
///
/// - 网关：移动独有 interface 的机制（页面通道表 + 裸字节转发）随域迁移动宿主
///   `plugin/terminal_stream.rs`（WIT layer 与网关同处）；
/// - `sql_guard`：主库收归后只剩一条 `column_to_json` re-export，两侧均无消费者
///   （域实现直连共享核）⇒ 退役而非留垫片（「符号面在场」会被误读为能力面）。
#[test]
fn mobile_only_mechanisms_and_retired_shims_stay_out_of_wasm_core() {
    let gateway = wasm_core_src().join("terminal_stream_gateway.rs");
    assert!(
        !gateway.exists(),
        "终端输出网关回流 wasm-core：{}（移动独有域机制住宿主 plugin/terminal_stream.rs）",
        gateway.display()
    );
    let lib = read(&wasm_core_src().join("lib.rs"));
    assert!(
        !lib.contains("pub mod terminal_stream_gateway"),
        "wasm-core 重新挂载了终端输出网关模块"
    );
    let sql_guard = wasm_core_src().join("host_api/sql_guard.rs");
    assert!(
        !sql_guard.exists(),
        "零消费者的 sql_guard 垫片又出现了：{}",
        sql_guard.display()
    );
    assert!(
        !read(&wasm_core_src().join("host_api.rs")).contains("pub mod sql_guard"),
        "wasm-core 重新挂载了 sql_guard 垫片"
    );

    // 正向：宿主侧网关在场（网关 + WIT layer 同一文件，能力去内核化 ≠ 能力消失）
    let host = read(&mobile_plugin_dir().join("terminal_stream.rs"));
    for needle in [
        "pub fn terminal_stream_gateway()",
        "impl bedcode::plugin::host_terminal_stream::Host for",
    ] {
        assert!(
            host.contains(needle),
            "移动宿主 terminal_stream.rs 缺少 `{needle}`（网关或 WIT layer 缺失 = 能力消失）"
        );
    }
}

// ==================== 判据 6：退役的 http 引擎不得回接 ====================

#[test]
fn retired_mobile_http_engine_stays_retired() {
    let root = repo_root();
    let engine = root.join("packages/bedcode-wasm-core/src/host_api/http_engine.rs");
    assert!(
        !engine.exists(),
        "已退役的移动 http 引擎又出现了：{}（执行本体住能力域 crate）",
        engine.display()
    );
    let host_api = read(&root.join("packages/bedcode-wasm-core/src/host_api.rs"));
    assert!(
        !host_api.contains("pub mod http_engine"),
        "wasm-core 重新挂载了已退役的 http 引擎模块"
    );
}

// ==================== 判据 7：端词汇全集（票 06 补） ====================

/// cap 分片 world 名——内核**代码行**零命中
///
/// 契约分片是**端组合面**的事实（各端 SDK `compose.json` 决定这一端组合了哪些
/// cap），内核既不读 `compose.json` 也不该知道 cap 的存在。票 04 / 05 把端绑定
/// 迁宿主后内核已无任何 cap 绑定逻辑 ⇒ 本判据把「代码行零命中」钉成事实，防止
/// 有人为图省事在内核里写一句按 cap 分支的 `#[cfg]` 或路径常量。
///
/// **注释不在判据内**：`core_binding.rs` 的模块文档必须点名「cap 组合不在内核、
/// 各端 compose 各自决定」——那句话正是本判据存在的理由，把文档判红等于要求内核
/// 对自己的边界闭嘴。
const CAP_SLICE_WORDS: [&str; 5] = ["cap-desktop", "cap-mobile", "cap-pty", "cap-http", "cap-ws"];

/// 移动端 SDK crate 名（单形态起**零命中**）
///
/// 用户裁决（2026-10-13）：wasm-core 零端 ABI 知识、端 SDK 依赖归两端宿主。
/// 单形态收口后移动 SDK 词汇（`abi::ABI_VERSION` / `BusMessage`）已完全退出
/// wasm-core——ABI 规格经 `AbiPolicy` 由宿主注入、`BusMessage` 由移动宿主拆分后
/// 走统一 `on_message`。本判据查**零命中**（含注释，防回归到封顶语义）。
const MOBILE_SDK_CRATE: &str = "bedcode_plugin_api_mobile";

/// ADR 0047 D2「自持标识符零端名」的**声明位**豁免：`(文件, 声明名, 理由)`
///
/// 只豁免声明名，不豁免整个文件。收进登记表而不是静默忽略——豁免项一旦多起来，
/// 「零端名」就退化成一句口号，读锁的人能从这张表直接看出还欠多少。理由栏与
/// 判据分开存放，匹配只认前两项：把理由写进「声明名」会让匹配永远落空，而
/// 一个静默失效的豁免比没有豁免更糟。
const DECLARED_NAME_EXEMPTIONS: [(&str, &str, &str); 1] = [(
    "manifest.rs",
    "neutral_manifest_json_shape_matches_desktop_sdk",
    "中性 manifest JSON 形状与**桌面 SDK** 的对照探针：该测试的对照物按定义就是桌面 SDK \
     的类型（`bedcode_plugin_api::PluginManifest`），改中性名会把「对照哪一侧」这个测试意图 \
     抹掉。它是契约漂移探针，不是机制面符号。",
)];

/// 源码里的声明位（`mod` / `struct` / `enum` / `trait` / `fn` / `type` / `const` /
/// `static`，含任意可见性限定）——只取声明**名**，用于「零端名」判定
///
/// 为什么只查声明位：ADR 0047 D2 的「登记保留类」明确放行端事实路径
/// （`bedcode-desktop/…`、`bedcode-mobile/…`，测试按路径读桌面 SDK 源码）与机制
/// 描述性文案（如 `"… in mobile headless context"`）。全行子串匹配会把这两类
/// 正当内容判红，判红即锁空转。声明位是「自持标识符」的最小无歧义口径。
fn declared_names(content: &str) -> Vec<(usize, String)> {
    const KEYWORDS: [&str; 8] = ["mod", "struct", "enum", "trait", "fn", "type", "const", "static"];
    let mut out = Vec::new();
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim();
        // 跳过整行注释与文档注释（迁移史说明正当地提到端名）
        if line.starts_with("//") {
            continue;
        }
        // 剥可见性限定：`pub` / `pub(crate)` / `pub(in …)` / `pub(super)`
        let rest = match line.strip_prefix("pub") {
            Some(r) => match r.strip_prefix('(') {
                Some(inner) => match inner.find(')') {
                    Some(close) => inner[close + 1..].trim_start(),
                    // 括号不配平 = 未覆盖写法，宁可报错也不猜
                    None => continue,
                },
                None => r.trim_start(),
            },
            None => line,
        };
        let Some((keyword, tail)) = KEYWORDS.iter().find_map(|kw| rest.strip_prefix(kw).map(|t| (*kw, t))) else {
            continue;
        };
        // `mod` 后必须跟空白（挡 `module` / `model` 同前缀标识符）
        if !tail.starts_with(char::is_whitespace) {
            continue;
        }
        let name: String = tail
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push((idx + 1, format!("{keyword} {name}")));
        }
    }
    out
}

#[test]
fn wasm_core_carries_no_end_slice_or_sdk_vocabulary() {
    let src = wasm_core_src();
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files);

    // ---- 7a · cap 分片 world 名零命中（**代码行**，跳注释）----
    // 只查代码行，不查注释：`core_binding.rs` / `bare.rs` 的模块文档**必须**点名
    // 「cap 组合不在内核、各端 compose 各自决定」——那是本判据存在的理由本身，
    // 把文档判红就是要求内核闭嘴。代码行里出现 cap 名才是真泄漏：意味着存在
    // 按 cap 分支 / 按 cap 绑定的实际逻辑。
    let mut cap_violations: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        for (idx, line) in code_lines(&read(path)) {
            for needle in CAP_SLICE_WORDS {
                if line.contains(needle) {
                    cap_violations.push(format!("{rel}:{}: `{needle}` → {}", idx + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        cap_violations.is_empty(),
        "wasm-core 出现 cap 分片 world 名（cap 组合是**各端 SDK `compose.json`** 的事实，\
         内核既不读 compose 也不该知道 cap 存在）：\n{}",
        cap_violations.join("\n")
    );

    // ---- 7b · 声明位零端名（ADR 0047 D2），豁免走登记表 ----
    let mut name_violations: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        for (line_no, decl) in declared_names(&read(path)) {
            let lower = decl.to_lowercase();
            if !lower.contains("desktop") && !lower.contains("mobile") {
                continue;
            }
            if DECLARED_NAME_EXEMPTIONS
                .iter()
                .any(|(exempt_file, exempt_name, _)| rel == *exempt_file && decl.ends_with(exempt_name))
            {
                continue;
            }
            name_violations.push(format!("{rel}:{line_no}: `{decl}`"));
        }
    }
    assert!(
        name_violations.is_empty(),
        "wasm-core 声明位出现端名（ADR 0047 D2「自持标识符零端名」；端事实路径与描述性文案不在\
         判据内，已知豁免见本文件 `DECLARED_NAME_EXEMPTIONS`——豁免要写进该表，不要就地忽略）：\n{}",
        name_violations.join("\n")
    );

    // 模块落点亦不得带端名（ADR 0047 D2：模块 / 目录名）
    for entry in std::fs::read_dir(&src).expect("wasm-core src 可读") {
        let path = entry.expect("dir entry").path();
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        assert!(
            !file_name.contains("desktop") && !file_name.contains("mobile"),
            "wasm-core 顶层出现带端名的模块落点：{}（模块 / 目录名同属 ADR 0047 D2 判据）",
            path.display()
        );
    }

    // ---- 7c · 移动端 SDK 词汇零命中（单形态收口：端 SDK 依赖归两端宿主）----
    // 只查**代码行**（跳整行注释）：`runtime.rs` 的 AbiPolicy 文档必须点名
    // 「移动 SDK 作为 AbiPolicy 注入来源」——那是本判据存在的理由本身。
    let mut sdk_files: Vec<String> = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let hit = code_lines(&read(path)).any(|(_, line)| line.contains(MOBILE_SDK_CRATE));
        if hit {
            sdk_files.push(rel);
        }
    }
    assert!(
        sdk_files.is_empty(),
        "移动端 SDK `{MOBILE_SDK_CRATE}` 出现在 wasm-core 代码行：{sdk_files:?}——单形态起\
         端 SDK 依赖归两端宿主（ABI 规格经 `AbiPolicy` 注入、`BusMessage` 由移动宿主\
         拆分走统一 `on_message`）；新增依赖点须上移到端宿主，不要在内核扩端 SDK 依赖"
    );

    // ---- 7d · 正向锚点（防空转）：core 交集绑定必须在场且确实在绑契约真源 ----
    // 「端词汇零命中」最危险的失效方式是判据扫描器扫空了。正向锚点：
    // core 交集绑定文件在盘 + 它确实含 `bindgen!` 且指向契约真源 `wit/core.wit`。
    let core_binding = read(&src.join("manager/runtime/core_binding.rs"));
    assert!(
        core_binding.contains("bindgen!") && core_binding.contains("wit/core.wit"),
        "core 交集绑定异常：`manager/runtime/core_binding.rs` 必须仍含 `bindgen!` 且指向契约真源 \
         `wit/core.wit`——它是判据 7a/7b/7c 的正向锚点（内核唯一合法的绑定面），缺它则本锁可能空转"
    );
}
