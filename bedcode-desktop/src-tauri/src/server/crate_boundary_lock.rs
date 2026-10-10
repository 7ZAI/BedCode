//! crate 边界锁（server-lib-split spec D7 / §4 票 06，由实施票 07 收口；
//! wasm-core-lib-split 票 09 把机制内核与两个能力域 crate 并入登记表）
//!
//! 票 03-05 已把结构锁逐面下沉进各自 crate（`bedcode-server-websocket` /
//! `-peer-net` 的 `dependency_direction_lock.rs`）。那些锁有一个共同盲区：
//! **每把锁只看得见自己与被点名的少数对侧**。例如 HTTP 面的清单里若被加上
//! `bedcode-server-peer-net`，没有任何一把面内锁会转红——而「传输面之间零横向」
//! 恰恰是被这些锁声称守住的不变量。
//!
//! 宿主 crate 是**唯一同时看得见全部拆分产物清单**的地方（且宿主 `cargo test`
//! 一定跑，而六把面内锁需要逐 crate 跑），所以「全图」这一层放在宿主侧：
//!
//! | 断言 | 守住什么 | 逐面锁为何覆盖不到 |
//! | --- | --- | --- |
//! | ① 拆分产物清单之间零横向 / 无反向依赖 | I1 + 「面之间零横向」的全矩阵 | 每把锁只钉自己 + 1~2 个对侧 |
//! | ② 必需的向下依赖边存在 | I2 / D1（base 是所有面的地基） | 缺边只在**使用者**侧可见 |
//! | ③ 宿主清单声明全部拆分产物 | 拆分产物没被悄悄摘掉 | 面内锁看不见宿主清单 |
//! | ④ 双面认识点唯一 | I1 在宿主侧的表达（装配面收敛在组合根） | 面内锁看不见宿主源码 |
//! | ⑤ 全局端口注册表只有一个装配点 | 组合根唯一性（票 07 的实测教训） | 无——纯宿主侧不变量 |
//!
//! **登记表为什么带路径**：机制内核 `bedcode-host-kit`、八个能力域 / 传输面 crate
//! （2026-10-07 能力域 lib 迁根）与整核本体 `bedcode-wasm-core`（2026-10-08 迁根）
//! 全部落**仓库根** `packages/`。只登记名字的表会
//! 默认同一个父目录，于是断言①会把它们报成「拆分产物缺失」或静默跳过——锁变成
//! 空转，而空转的锁比没有锁更危险。
//!
//! ## 为什么 ④ / ⑤ 值得单独锁（两次实测教训）
//!
//! - **④**：两个面各自抽 crate 后，I1 的执行方式从「源码文本锁」变成「靠注入而非
//!   引用」。一旦宿主里多出第二个同时认识两面 crate 的生产模块，就说明有人在组合根
//!   之外偷偷接了一根线——那正是票 03 让 `serve()` 只认 `TransportFace` 想消灭的形态。
//! - **⑤**：`bedcode_server_base::ports` 是进程级 `OnceLock`。曾经 GUI bootstrap 装
//!   它、无头 harness 不装，症状是网关 `ports::get() == None` 判 `PassThrough` →
//!   插件激活期登记的**全部**宿主别名 404，而不依赖端口的路径（端口占用、WS 升级）
//!   照常工作：七个跨端场景同时红，且看上去像协议 bug。锁把「装配点唯一」变成可执行
//!   判据后，新增 harness 必须走组合根的 `install_server_ports()`，无法再内联装配。
//!
//! 本锁只读磁盘文本，不引入任何生产依赖；锁自身文件不参与源码扫描（它以字符串形式
//! 携带全部禁用字面量）。

use std::path::{Path, PathBuf};

/// 拆分产物清单：`(crate 名, 相对仓库根 `packages/` 的路径)`
///
/// **单一事实源已上提 `bedcode-wasm-core` crate**（wasm-core-whole-crate 票 05
/// 收口：lib → crate 单向引用）。本表是那张登记表的**再导出**，不是第二份拷贝——
/// 新增/改名拆分产物只改 `bedcode_wasm_core::crate_boundary_lock::SPLIT_CRATES`
/// 一处（曾为镜像表，两处同改的漂移风险已消除）。
///
/// 表内容：server 五面 + 加密引擎（票 03-06）+ 机制内核（仓库根双端共享锚点）+
/// 能力域 crate（wasm-core-lib-split 票 03/07/08）+ **整核本体 `bedcode-wasm-core`**
/// （wasm-core-whole-crate 票 02/05；2026-10-08 迁根）。ADR 0036 撤销的 `bedcode-sqlite-engine`
/// 不在表内（`host-database` / `host-plugin-database` / `host-storage` 三域与
/// SQLite 引擎面留在核心）。
///
/// **为什么带路径列**：机制内核、能力域 / 传输面 crate 与整核本体
/// `bedcode-wasm-core` 全部落**仓库根** `packages/`（2026-10-07 迁 8 个能力域 +
/// 2026-10-08 迁整核本体）。表只给名字时，锁必须假定同一个
/// 父目录——那种假定正是「夹具落到第二个 target 落点」那类事故的配方。
pub(crate) use bedcode_wasm_core::crate_boundary_lock::SPLIT_CRATES;

/// 拆分产物的 crate 名（登记表的投影）
fn split_crate_names() -> Vec<&'static str> {
    SPLIT_CRATES.iter().map(|(name, _)| *name).collect()
}

/// 拆分产物目录（按登记表解析路径）
fn split_crate_dir(crate_name: &str) -> PathBuf {
    let (_, rel) = SPLIT_CRATES
        .iter()
        .find(|(name, _)| *name == crate_name)
        .unwrap_or_else(|| panic!("拆分产物清单缺 crate `{crate_name}`——新增/改名时必须同改本表"));
    packages_dir().join(rel)
}

/// 宿主 crate 名——出现在任何拆分产物清单里即「反向依赖宿主」，在 crate
/// 边界层面直接不成立（票 05/06 的面内锁也各自禁这一条，本锁在全矩阵上再钉一次）
const HOST_CRATE: &str = "bedcode-desktop";

/// **登记表**：允许的向下依赖边（key crate → 它可以依赖的拆分产物兄弟）
///
/// 形状即 server-lib-split spec §2 的依赖图：`base` 是叶子地基；`crypto-engine` 与
/// `core` 向上取地基；两个传输面只向下取 base + core；`peer-net` 是**独立引擎域**
/// （D2）——它不依赖 core，因为它对内核零引用，依赖 core 只是「同在 server 下」
/// 的错觉。
///
/// 形状即 spec §2 的依赖图：`base` 是叶子地基；`crypto-engine` 与 `core` 向上取
/// 地基；两个传输面只向下取 base + core；`peer-net` 是**独立引擎域**（D2）——
/// 它不依赖 core，因为它对内核零引用，依赖 core 只是「同在 server 下」的错觉。
///
/// 新增边 = 改动架构，必须改本表并在 spec / ADR 记理由（登记表语义）。
///
/// `http → crypto-engine` 是票 08 新增的一条，**仅 `[dev-dependencies]`**（生产段
/// 仍只有 base + core，断言 ② 钉着）：链路加密集成用例要扮演客户端、自建 x25519
/// 对端密钥，生产代码一律经 core 的 `link_crypto` 取能力。判据对 `[dependencies]`
/// 与 `[dev-dependencies]` 一视同仁（横向边在 dev 段也是横向），所以这条边必须显式
/// 登记而不是「顺手加上去」。
///
/// wasm-core-lib-split 追加的边（票 03/04/05/06）：`host-kit` 不依赖任何拆分
/// 产物（机制内核自身零内部依赖）；其余拆分产物里，**凡带插件绑定层的**都必须站在
/// 它上面——`add_to_linker::<WasmPluginState, D>` 的单态 `S` 在 kit 里，而绑定层就是
/// 「实现搬进 crate 后仍要被装配」的那一半。传输面（http / websocket / peer-net）
/// 与能力域 crate（discovery）同此。票 07/08 的 sqlite 能力域 crate 已由 ADR 0036
/// 整体撤销，故不在本表内。
///
/// **能力域之间、传输面之间、各域与传输面之间仍全部零横向**（各能力域之间不许互相
/// 引用，否则会耦成一体，违背「按需组合不同核心」的前提）。
const ALLOWED_DOWNWARD_EDGES: &[(&str, &[&str])] = &[
    ("bedcode-server-base", &[]),
    ("bedcode-crypto-engine", &["bedcode-server-base"]),
    ("bedcode-server-core", &["bedcode-server-base", "bedcode-crypto-engine"]),
    (
        "bedcode-server-http",
        &[
            "bedcode-server-base",
            "bedcode-server-core",
            "bedcode-crypto-engine",
            "bedcode-host-kit",
        ],
    ),
    (
        "bedcode-server-websocket",
        &["bedcode-server-base", "bedcode-server-core", "bedcode-host-kit"],
    ),
    ("bedcode-server-peer-net", &["bedcode-server-base", "bedcode-host-kit"]),
    // 机制内核：不依赖任何拆分产物（机制面零内部依赖）
    ("bedcode-host-kit", &[]),
    // 能力域 crate：向下只取机制内核
    ("bedcode-discovery-engine", &["bedcode-host-kit"]),
    // host-pty 能力域（票 02 迁引擎体 → ADR 0039 整面迁出）：引擎面 + WIT 接线 +
    // 域机制都住在本 crate（自带 `bindgen!` + `HostModule` 自报）；端口 adapter 自
    // 票 02 批次 02 起住宿主 `src/plugin/pty.rs`（宿主直接消费，wasm-core 不再依赖
    // 本 crate）。向下边与四域同形：基础层（错误 / 常量）+ 机制内核 host-kit
    // （`WasmPluginState` / `HostModule` / `ModuleEntry` 是 `add_to_linker` 与自报
    // 的前提，与 http / ws / peer-net / discovery 完全一致）。
    ("bedcode-pty-engine", &["bedcode-server-base", "bedcode-host-kit"]),
    // 整核本体（wasm-core-whole-crate 票 05）：bedcode-wasm-core 是全部拆分产物
    // 的组装点——站在基础层（base / crypto / host-kit / server-core）与全部能力域
    // crate（mdns / ws / peer / http）之上，把四通道与四闸门装配成一个可复用的
    // 插件机制内核。它依赖 `bedcode-plugin-api`（SDK WIT 绑定）与根 `packages/`
    // 的 `bedcode-peer-net` / `bedcode-link-crypto`，但后两者不在拆分产物登记表内
    // （第三方 / 独立 crate，不归本锁管辖面）。
    (
        "bedcode-wasm-core",
        &[
            "bedcode-server-base",
            "bedcode-server-core",
            "bedcode-crypto-engine",
            "bedcode-host-kit",
            "bedcode-server-websocket",
            "bedcode-server-peer-net",
            "bedcode-server-http",
            // 票 02 批次 02：`bedcode-pty-engine` 边已摘除（端口 adapter 随域迁宿主
            // `src/plugin/pty.rs` 且内核不再引用该 crate）。
            //
            // **peer / ws / http 三条边仍是保留边**（本清单 + 下方 REQUIRED 表同时登记）：
            // 批次 03 只把它们的**端口 adapter** 迁宿主（`src/plugin/{peer,...}.rs`），
            // 内核继续消费这些 crate 的**引擎面**（peer 的 `release_node_for` / ws / http
            // 的帧投递与连接清单）——摘边要把那些消费点一并搬出内核，属另一件事。
            //
            // **票 06 批次 03 新增 `bedcode-discovery-engine` 边（mobile-host 面）**：
            // 桌面形态该域 adapter 在宿主 `src/plugin/mdns.rs`（内核无边）；移动形态无
            // 宿主 adapter——fork 迁入的 `mobile/host_impl` 域 impl 在 crate 内直接消费
            // discovery-engine 引擎面（装配形态差异，非桌面回接）。此后再出现
            // wasm-core → 能力域的**新**边即红（反向/横向同理）。
            "bedcode-discovery-engine",
        ],
    ),
];

/// 必需的向下依赖边（只看生产 `[dependencies]` 段）
///
/// 缺失即红：要么有人把内核面复制进了本 crate，要么清单被误删——两种都是票 02
/// 明确禁止的形态。`base` 出现在每一条里，因为它是端口 traits / 错误 / 网络配置
/// 的定义处（`ports::get()` 唯一的合法来源）。
const REQUIRED_DOWNWARD_EDGES: &[(&str, &str)] = &[
    ("bedcode-crypto-engine", "bedcode-server-base"),
    ("bedcode-server-core", "bedcode-server-base"),
    ("bedcode-server-core", "bedcode-crypto-engine"),
    ("bedcode-server-http", "bedcode-server-base"),
    ("bedcode-server-http", "bedcode-server-core"),
    ("bedcode-server-websocket", "bedcode-server-base"),
    ("bedcode-server-websocket", "bedcode-server-core"),
    ("bedcode-server-peer-net", "bedcode-server-base"),
    // 能力域 crate 必须真的站在机制内核上（`add_to_linker::<WasmPluginState, D>`
    // 的单态 S 在 kit 里；缺这条边说明绑定层搬错了位置）
    ("bedcode-discovery-engine", "bedcode-host-kit"),
    // host-pty 能力域（票 02 + ADR 0039）：生产 `[dependencies]` 必须有基础层
    // （错误 / 常量 / 连接身份真源在 bedcode-server-base；缺边 = 引擎复制了错误
    // 类型或清单被误删）与机制内核（host-kit：WIT 接线 + 自报的前提，同四域）
    ("bedcode-pty-engine", "bedcode-server-base"),
    ("bedcode-pty-engine", "bedcode-host-kit"),
    // 同理：三个传输面各自持一份 `plugin_binding`（域实现 + 自报），也必须站在
    // 机制内核上。缺边 ⇒ 绑定层要么被搬回了宿主，要么那域根本没进 crate。
    ("bedcode-server-http", "bedcode-host-kit"),
    ("bedcode-server-websocket", "bedcode-host-kit"),
    ("bedcode-server-peer-net", "bedcode-host-kit"),
    // 整核本体（wasm-core-whole-crate 票 05）：必须真的站在基础层与全部能力域之上
    // （缺 host-kit ⇒ 机制内核被复制进本 crate；缺能力域 crate ⇒ 能力实现被复制进
    // 内核——两种都是票 02 明令禁止的形态）。
    ("bedcode-wasm-core", "bedcode-server-base"),
    ("bedcode-wasm-core", "bedcode-host-kit"),
    ("bedcode-wasm-core", "bedcode-crypto-engine"),
    ("bedcode-wasm-core", "bedcode-server-core"),
    ("bedcode-wasm-core", "bedcode-server-websocket"),
    ("bedcode-wasm-core", "bedcode-server-peer-net"),
    ("bedcode-wasm-core", "bedcode-server-http"),
    // 票 02 批次 02：wasm-core → pty-engine 必须边已摘除（连同内核侧 adapter 一起
    // 消失）；ws / peer-net / http 三条边因内核仍用其引擎面（bus 帧投递 / 连接清单 /
    // 端点登记）保留。discovery-engine 不进必填段：它是 mobile-host 形态的可选边
    // （桌面形态走宿主 adapter，不上该依赖；见上方 ALLOWED 表注释）。
];

/// 宿主源码里**允许**同时认识两个传输面 crate 的文件（断言 ④ 的登记表）
///
/// 只有组合根：`bedcode_server_core::app::serve` 只认 `TransportFace`，两个面的
/// face 实现各自随面下沉，把 faces 交给 `serve` 这一步只有一处。新增条目必须同时
/// 回答「为什么不能在组合根之外接线」。
const BOTH_FACES_ALLOWLIST: &[&str] = &["src/server/composition.rs"];

/// 宿主源码里**允许**调 `ports::init`（装全局端口注册表）的文件（断言 ⑤ 的登记表）
///
/// 只有组合根的 `install_server_ports()`。测试二进制里**也**不许内联装配——它们
/// 必须调同一个函数，否则「GUI 装配了、harness 没装配」这类分裂会以静默 404 的形式
/// 复发（无头 rig 正是这样红过一次）。`ports_impl::assemble()` 本身在
/// `peer_net_cmds.rs` 有正当消费者（端口未装配时的兜底 `PeerCtx`），不在本表管辖内。
const PORTS_INIT_CALL_SITE_ALLOWLIST: &[&str] = &["src/server/composition.rs"];

// ==================== 通用判据（契约例与锁本体共用） ====================

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// 宿主 crate 根（`<desktop>/src-tauri`）
fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 拆分产物基准目录（仓库根 `packages/`；2026-10-08 整核本体迁根后全部拆分产物
/// 同落此处，登记表内条目即裸 crate 名）
fn packages_dir() -> PathBuf {
    crate_root().join("..").join("..").join("packages")
}

/// 拆分产物的 `src` 扫描根（供宿主的退役面防回接锁共用）
///
/// **实现已收口为单向引用**（wasm-core-whole-crate 票 05）：扫描根由
/// `bedcode_wasm_core::crate_boundary_lock` 提供（单一事源），本函数只是
/// 薄委托——面抽 crate 后，宿主那些「扫 `src` 树」的退役面锁
/// （`retired_kernel_session_domain_*` / `retired_peer_transfer_orchestration_*` /
/// `retired_session_observation_*` / 会话命令面锁）必须把 crate 也扫进去，否则退役面
/// 被回接到面 crate 里时宿主锁全绿。它们共享登记表而不是各自抄一份，避免两处漂移。
pub(crate) fn server_lib_src_roots() -> Vec<PathBuf> {
    bedcode_wasm_core::crate_boundary_lock::server_lib_src_roots()
}

fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("crate 边界锁无法读取 {}：{e}", path.display()))
}

/// 逐行解析清单，返回 `(section 名, 依赖键)` 对
///
/// 按**行**切段：段内值常含 `features = ["derive"]` 这类方括号，用「下一个 `[`」
/// 找段尾会把清单截断在 serde 行上（票 05 首版实测踩中，误报「清单缺少内核依赖」）。
/// `[target.'cfg(x)'.dependencies]` 这类带条件的段按**后缀**匹配段名。
fn parse_dependency_keys(manifest: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    for raw in manifest.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current = trimmed.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }
        let Some((key, _)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        if !key.is_empty() {
            out.push((current.clone(), key));
        }
    }
    out
}

/// 取指定依赖段（含 dev / build）的依赖键集合
///
/// 横向边在**任何**段里都是横向边（dev-dependency 拉来另一个传输面，测试里
/// 就能看见它的内部形状）；段名按后缀匹配，`dependencies` 本身也命中
/// `dev-dependencies` 的后缀判定，故调用方需精确指定段名。
fn dep_keys(manifest: &str, sections: &[&str]) -> Vec<String> {
    parse_dependency_keys(manifest)
        .into_iter()
        .filter(|(section, _)| sections.iter().any(|want| section == want))
        .map(|(_, key)| key)
        .collect()
}

/// 找独立段：前为 `::`、后非标识符字节
fn find_segment_positions(src: &str, segment: &str) -> Vec<usize> {
    let bytes = src.as_bytes();
    let mut positions = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find(segment) {
        let start = search + rel;
        let end = start + segment.len();
        let before_ok = start >= 2 && bytes[start - 2] == b':' && bytes[start - 1] == b':';
        let after_ok = bytes.get(end).map_or(true, |&b| !is_ident_byte(b));
        if before_ok && after_ok {
            positions.push(start);
        }
        search = start + 1;
    }
    positions
}

/// 词边界匹配（crate 名是裸标识符，前面没有 `::`）
fn find_word_positions(src: &str, needle: &str) -> Vec<usize> {
    let bytes = src.as_bytes();
    let mut positions = Vec::new();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find(needle) {
        let start = search + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = bytes.get(end).map_or(true, |&b| !is_ident_byte(b));
        if before_ok && after_ok {
            positions.push(start);
        }
        search = start + 1;
    }
    positions
}

/// `seg_start` 处段名的前一段名（其前应为 `::`；UTF-8 多字节相邻时返回 None 且不 panic）
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
    src.get(j..seg_start - 2)
}

fn line_number(src: &str, pos: usize) -> usize {
    src[..pos].bytes().filter(|b| *b == b'\n').count() + 1
}

fn line_content(src: &str, pos: usize) -> String {
    let start = src[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = src[pos..].find('\n').map(|i| pos + i).unwrap_or(src.len());
    src[start..end].trim_end_matches('\r').to_string()
}

fn format_hits(hits: &[(String, usize, String)]) -> String {
    hits.iter()
        .map(|(f, l, c)| format!("  {f}:{l}: {c}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 是否属于测试面（路径含 `tests` 目录段 / `tests.rs` 模块入口，或文件名以 `_test.rs` 结尾）
///
/// 断言 ④ 只管**生产**源码：测试二进制与 `#[cfg(test)]` 树有权同时驱动两个面
/// （`session_e2e` 就要经面 crate 的 DTO 造黄金形状），把它们算进来只会逼出一堆
/// 无意义的豁免条目，稀释「登记 = 显式裁决」的语义。`tests.rs` 与 `tests/` 同名同义
/// （AGENTS §6 模块入口文件与目录同名），一并视为测试面。
fn is_test_tree(rel: &str) -> bool {
    rel.split('/').any(|seg| matches!(seg, "tests" | "tests.rs")) || rel.ends_with("_test.rs")
}

/// 递归枚举宿主 `src/**/*.rs`（排除本锁文件），返回相对 crate 根的正斜杠路径
fn collect_host_src_files() -> Vec<String> {
    let root = crate_root().join("src");
    let mut stack = vec![root];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let rel = path
                    .strip_prefix(crate_root())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel.ends_with("crate_boundary_lock.rs") {
                    continue;
                }
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// 收集宿主集成测试二进制目录（`src-tauri/tests/**/*.rs`）的相对路径
///
/// 断言 ⑤ 特意覆盖这一面：宿主自己的集成 harness 曾各自内联端口装配，收敛到组合根
/// 后它们必须调同一函数。
fn collect_host_itest_files() -> Vec<String> {
    let root = crate_root().join("tests");
    let mut stack = vec![root];
    let mut out = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                let rel = path
                    .strip_prefix(crate_root())
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel.ends_with("crate_boundary_lock.rs") {
                    continue;
                }
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// 源码里「某 crate 名以裸标识符形态出现」的命中（`use bedcode_server_http::…`
/// 形态里 crate 名前面**没有** `::`，故只能用词边界判据——票 05/06 的面内锁在
/// 这点上都栽过一次：拿段判据去扫 crate 名，实测恒零命中、锁静默失效）
fn find_crate_name_hits(files: &[String], crate_ident: &str) -> Vec<(String, usize, String)> {
    let mut hits = Vec::new();
    for rel in files {
        let src = read_file(&crate_root().join(rel));
        for pos in find_word_positions(&src, crate_ident) {
            hits.push((rel.clone(), line_number(&src, pos), line_content(&src, pos)));
        }
    }
    hits.sort();
    hits
}

/// 命中里的**文件集合**，**排除**纯强制引用行（`use <crate> as _;`）
///
/// 为什么排除（断言 ④ 的精度修正，wasm-core-lib-split 票 06 实测触发）：
/// `inventory` 的能力模块自报靠 linker-section 静态，**未被引用的 rlib 不进最终
/// 二进制**⇒ 宿主必须有一行 `use <crate> as _;` 强制引用每个能力域所在的 crate。
/// 能力域每迁出一个 crate（票 04 ws / 05 peer-net / 06 http），这行就把该 crate 名
/// 写进了同一个文件，于是「同时认识两面」的机械扫描会把**能力模块注册面**误判成
/// 「传输面接线」。
///
/// 该形态**可证明无害**：`use X as _;` 只把 crate 根绑定为匿名别名，此后无法通过它
/// 取任何路径、类型或函数（Rust 里匿名绑定不可被引用），故它既不是「接线」也不构成
/// 对该 crate 的任何使用——真正的接线一定表现为 `bedcode_server_x::…` 形态的代码，
/// 那仍由本锁扫到。本函数只放行这一种逐字形态，任何别的引用照旧计入。
fn crate_name_hit_files<'a>(hits: &'a [(String, usize, String)]) -> std::collections::BTreeSet<&'a String> {
    hits.iter()
        .filter(|(_, _, line)| {
            let line = line.trim();
            !(line.starts_with("use ") && line.ends_with(" as _;"))
        })
        .map(|(f, _, _)| f)
        .collect()
}

/// 源码里「`foo::bar` 形态」的命中（判据：段 `bar` 的前一段是 `foo`）
fn find_qualified_call_hits(files: &[String], callee: &str, owner: &str) -> Vec<(String, usize, String)> {
    let mut hits = Vec::new();
    for rel in files {
        let src = read_file(&crate_root().join(rel));
        for pos in find_segment_positions(&src, callee) {
            if previous_segment(&src, pos) == Some(owner) {
                hits.push((rel.clone(), line_number(&src, pos), line_content(&src, pos)));
            }
        }
    }
    hits.sort();
    hits
}

fn allowed_for(crate_name: &str) -> &[&str] {
    ALLOWED_DOWNWARD_EDGES
        .iter()
        .find(|(name, _)| *name == crate_name)
        .map(|(_, allowed)| *allowed)
        .unwrap_or_else(|| panic!("登记表缺 crate `{crate_name}`——拆分产物新增/改名时必须同改本表"))
}

const ALL_DEP_SECTIONS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

// ==================== 锁本体 ====================

/// 断言①：拆分产物之间零横向、零反向依赖（全矩阵）
#[test]
fn server_lib_manifests_have_no_lateral_or_upward_edges() {
    let registry = split_crate_names();
    for crate_name in &registry {
        let manifest_path = split_crate_dir(crate_name).join("Cargo.toml");
        assert!(
            manifest_path.exists(),
            "拆分产物缺失：{}（{crate_name}）——被从 packages/ 摘掉了？",
            manifest_path.display()
        );
        let manifest = read_file(&manifest_path);
        let deps = dep_keys(&manifest, ALL_DEP_SECTIONS);

        for dep in &deps {
            assert!(
                dep != HOST_CRATE,
                "反向依赖违反：{crate_name} 的依赖清单声明了宿主 crate `{HOST_CRATE}`\
                 ——「面认识宿主」在 crate 边界层面就不成立（能力一律经 ports 端口反向要）"
            );
            if dep == crate_name {
                continue; // 自引用（改名期过渡）不算横向边
            }
            if !registry.contains(&dep.as_str()) {
                continue; // 第三方 crate 不在本锁管辖面（清单里绝大多数依赖是它们）
            }
            let allowed = allowed_for(crate_name);
            assert!(
                allowed.contains(&dep.as_str()),
                "横向/越级边违反：{crate_name} 依赖了兄弟 crate `{dep}`\
                 （允许的向下边只有 {allowed:?}）——传输面之间必须零横向，\
                 引擎域（peer-net）不得被传输面取用"
            );
        }
    }
}

/// 断言②：必需的向下依赖边存在（只看生产段）
#[test]
fn required_downward_edges_exist_in_production_deps() {
    for (crate_name, required) in REQUIRED_DOWNWARD_EDGES {
        let manifest_path = split_crate_dir(crate_name).join("Cargo.toml");
        let manifest = read_file(&manifest_path);
        let prod = dep_keys(&manifest, &["dependencies"]);
        assert!(
            prod.iter().any(|k| k == required),
            "I2/向下依赖违反：{crate_name} 的生产依赖清单缺少 `{required}`\
             （当前生产依赖：{prod:?}）——要么内核/基础面被复制进本 crate，要么清单被误删"
        );
    }
}

/// 断言③：宿主清单声明全部拆分产物
#[test]
fn host_manifest_declares_every_server_lib_crate() {
    let manifest = read_file(&crate_root().join("Cargo.toml"));
    let deps = dep_keys(&manifest, ALL_DEP_SECTIONS);
    for crate_name in split_crate_names() {
        assert!(
            deps.iter().any(|k| k == crate_name),
            "宿主依赖清单缺少 `{crate_name}`——拆分产物没被接线（表：{SPLIT_CRATES:?}）"
        );
    }
}

/// 断言④：双面认识点在宿主生产源码里唯一（组合根）
#[test]
fn composition_root_is_the_only_module_knowing_both_faces() {
    let files = collect_host_src_files();
    assert!(
        files.len() >= 20,
        "结构锁空转：宿主 src 只枚举到 {} 个 .rs（下限 20）——路径错或 read_dir 异常被吞",
        files.len()
    );

    let production: Vec<String> = files.into_iter().filter(|rel| !is_test_tree(rel)).collect();

    let http_hits = find_crate_name_hits(&production, "bedcode_server_http");
    let ws_hits = find_crate_name_hits(&production, "bedcode_server_websocket");

    let http_set: std::collections::BTreeSet<&String> = crate_name_hit_files(&http_hits);
    let ws_set: std::collections::BTreeSet<&String> = crate_name_hit_files(&ws_hits);
    let both: Vec<&String> = http_set.intersection(&ws_set).copied().collect();

    let unexpected: Vec<&&String> = both
        .iter()
        .filter(|f| !BOTH_FACES_ALLOWLIST.contains(&f.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "I1 在宿主侧被绕过：组合根之外出现了同时认识两个传输面 crate 的生产模块 —— \
         `serve()` 只认 TransportFace，faces 的接线必须只在组合根发生：\n{}",
        unexpected
            .iter()
            .map(|f| format!("  {}", crate_root().join(f).display()))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 登记表本身不得为空转：组合根必须真的同时认识两面
    assert!(
        !both.is_empty(),
        "结构锁空转：没有任何生产模块同时认识两个传输面 crate（登记表 {} 悬空）\
         ——组合根被摘掉或改名了",
        BOTH_FACES_ALLOWLIST.join(", ")
    );
}

/// 断言⑤：全局端口注册表只有一个装配点（组合根），生产源码与集成 harness 皆然
#[test]
fn server_ports_registry_has_exactly_one_install_call_site() {
    let mut files = collect_host_src_files();
    let itests = collect_host_itest_files();
    assert!(
        !itests.is_empty(),
        "结构锁空转：宿主集成测试目录枚举到 0 个 .rs（tests/ 路径错？）"
    );
    files.extend(itests);

    let hits = find_qualified_call_hits(&files, "init", "ports");
    // 组合根自身必然命中（它就是装配点），先剔除再判剩余
    let unexpected: Vec<(String, usize, String)> = hits
        .into_iter()
        .filter(|(rel, _, _)| !PORTS_INIT_CALL_SITE_ALLOWLIST.contains(&rel.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "全局端口注册表出现了组合根之外的装配点（票 07 实测：无头 rig 不装端口 → \
         网关判 PassThrough → 插件登记的宿主别名全 404，而依赖端口的路径照常工作，\
         症状伪装成协议 bug）：\n{}",
        format_hits(&unexpected)
    );

    assert!(
        PORTS_INIT_CALL_SITE_ALLOWLIST.contains(&"src/server/composition.rs"),
        "登记表缺组合根本身——`install_server_ports` 被摘掉或改名时必须同改本表"
    );
}

/// peer 上下文的**兜底装配**必须带宿主句柄（2026-10-07 实机回归的结构锁）
///
/// 端口注册表在 `PluginHost::new` **之后**才装，而插件激活发生在 `PluginHost::new`
/// 内部——file-transfer 的 `host-peer.start-node` 正落在那个窗口。兜底装配若不带句柄，
/// 路径面只能读尚未注册的 `AppContext` → `resolve app data dir failed: no runtime
/// context` → peer 节点起不来 → 桌面端不广播 → 移动端发现不到桌面。
#[test]
fn peer_ctx_fallback_assembly_carries_app_handle() {
    let rel = "src/server/peer_net_cmds.rs";
    let src = read_file(&crate_root().join(rel));
    let calls: Vec<(usize, String)> = find_segment_positions(&src, "assemble")
        .into_iter()
        .map(|pos| (line_number(&src, pos), line_content(&src, pos)))
        // 注释行不算调用点（只判真实代码行，避免文档里提到装配就被误伤）
        .filter(|(_, line)| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .collect();
    assert!(!calls.is_empty(), "结构锁空转：`{rel}` 里找不到 `assemble` 调用");

    let handle_less: Vec<(String, usize, String)> = calls
        .into_iter()
        .filter(|(_, line)| !line.contains("Some("))
        .map(|(line, content)| (rel.to_string(), line, content))
        .collect();
    assert!(
        handle_less.is_empty(),
        "`{rel}` 的兜底装配必须传句柄（`assemble(Some(app.clone()))`）——\
         不传时窗口内的路径解析恒定失败，peer 节点起不来：\n{}",
        format_hits(&handle_less)
    );
}

// ==================== 判据自身的契约例（防匹配规则被改坏后假绿） ====================

/// 清单解析：段内方括号不得截断；dev / build 段各自成段
#[test]
fn manifest_parsing_is_section_accurate() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n\
                     serde = { version = \"1\", features = [\"derive\"] }\n\
                     bedcode-server-core = { path = \"../bedcode-server-core\" }\n\n\
                     [dev-dependencies]\nbedcode-server-websocket = \"1\"\n\n\
                     [build-dependencies]\nbedcode-server-base = \"1\"\n";
    let prod = dep_keys(manifest, &["dependencies"]);
    assert!(
        prod.iter().any(|k| k == "bedcode-server-core"),
        "方括号截断了清单：{prod:?}"
    );
    assert!(
        !prod.iter().any(|k| k == "bedcode-server-websocket"),
        "dev 段污染了生产段：{prod:?}"
    );

    let all = parse_dependency_keys(manifest);
    assert_eq!(all.len(), 5, "全段收集漏项或多见：{all:?}");
    assert!(all
        .iter()
        .any(|(s, k)| s == "dev-dependencies" && k == "bedcode-server-websocket"));
    assert!(all
        .iter()
        .any(|(s, k)| s == "build-dependencies" && k == "bedcode-server-base"));
    // `[package]` 段的 `name = "x"` 不是依赖键：按段取依赖时不得混进来
    assert!(!prod.iter().any(|k| k == "name"), "[package] 段混进了依赖键：{prod:?}");
}

/// 测试面判定：路径含 `tests` 段或 `_test.rs` 结尾
#[test]
fn test_tree_classification_covers_dir_and_file_conventions() {
    assert!(is_test_tree("wasm_core/manager/runtime/tests/session_e2e.rs"));
    assert!(is_test_tree("wasm_core/manager/host/tests/l2_gating_test.rs"));
    assert!(is_test_tree("wasm_core/manager/host/tests.rs"));
    assert!(!is_test_tree("server/composition.rs"));
    assert!(!is_test_tree("wasm_core/manager/host/api_bridge.rs"));
    // 同名但不是段的路径不算（`latest/` 之类）
    assert!(!is_test_tree("utils/latest/x.rs"));
}

/// 段 / 词边界判定
#[test]
fn segment_and_word_boundaries_are_respected() {
    let src = "use bedcode_server_http::gateway;\nlet _ = bedcode_server_http::registry::count();\n\
                use bedcode_server_httpx::y;\nuse actix_web::http::header;\n";
    // crate 名在 `use` 后是**裸标识符**（前面没有 `::`）→ 只判两侧
    assert_eq!(find_segment_positions(src, "bedcode_server_http").len(), 0);
    assert_eq!(find_word_positions(src, "bedcode_server_http").len(), 2);

    // 段判定：粘连的 `::http_filter::` 不算 `::http::`
    let seg = "use crate::server::http_filter::No;\n";
    assert!(find_segment_positions(seg, "http").is_empty());
    assert_eq!(find_segment_positions(seg, "http_filter").len(), 1);

    // `foo::ports::init` 命中；`foo::ports::init_all` 不命中
    let call = "ports::init(x);\nports::init_all(y);\nother.ports::init(z);\n";
    let mut hits = Vec::new();
    for pos in find_segment_positions(call, "init") {
        if previous_segment(call, pos) == Some("ports") {
            hits.push(pos);
        }
    }
    assert_eq!(hits.len(), 2, "`ports::init` 段判定漂移：{hits:?}");

    // UTF-8 多字节相邻不得 panic
    assert_eq!(previous_segment("（http", 3), None);
    assert_eq!(previous_segment("http", 0), None);
}
