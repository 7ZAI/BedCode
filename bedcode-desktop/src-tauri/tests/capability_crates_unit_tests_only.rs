//! 拆分产物 crate 的**单测纯净性**锁：crate 只保留单元测试，跨 crate 集成测试归宿主
//!
//! ## 判据（一条规则，两个可测形态）
//!
//! 拆分产物（`bedcode-desktop/packages/bedcode-*`）是**可复用引擎 crate**，不是应用。
//! 一个 crate 根的 `tests/` 目录 = 一个独立测试二进制 = 一个只能经 `pub` API 访问的
//! **对外行为面**。它带来两重代价：
//!
//! 1. **纯净性代价**：crate 的对外行为面理应由宿主验（宿主是唯一装配全部 crate 的地方），
//!    crate 自己开 `tests/` 等于把「我与谁组合才对」这件事写进自己的仓库。
//! 2. **依赖图代价**：`tests/` 只能经 `[dev-dependencies]` 追加依赖，而 **dev 边在依赖图里
//!    就是一条真边**——单向引用即可编译，是真实可发生的越线形态（`bedcode-server-http`
//!    曾为链路加密 HTTP 全链路用例 dev-depend `bedcode-crypto-engine`，生产清单里并没有它）。
//!
//! 故本锁钉两条：① 治理 crate 不得有 crate 根 `tests/`（连带 `benches/`、`examples/`、
//! `[[test]]`/`[[bench]]`/`[[example]]` 段——它们是同一类「可编译的对外测试面」的别名）；
//! ② 治理 crate 的 `[dev-dependencies]` 不得含**任何内部 crate**（`bedcode*` 前缀）。
//! 外部 dev 依赖（`tempfile` / `tracing-subscriber` / `tokio` feature 等）不在此列——
//! 它们服务的是 `src/` 内的单元测试夹具，不是跨 crate 组合。
//!
//! ## 为什么已有锁不够
//!
//! - 各传输面的 `dependency_direction_lock.rs` 明确**只解析 `[dependencies]`**（并专门断言
//!   dev-dependencies 不得污染判定），故对 dev 横向边零覆盖。
//! - `crate_boundary_lock`（`SPLIT_CRATES` 登记表）管的是「边是否合法」「`src` 扫描根在哪」，
//!   不看 crate 有没有多出 `tests/` 目录。
//! - `capability_crates_no_product_ids.rs` 管的是**语义**（产品 id / 退役面词汇），
//!   且它按「路径含 `tests` 段即纯测试文件」把 `tests/` 目录**整个排除在扫描面外**——
//!   正好放行了本锁要禁的形态。两条锁互补，都不能删。
//!
//! 本次清退的两个实例（都来自 server-lib-split 票 08 的迁移裁决）：
//! `bedcode-server-http/tests/link_crypto_http.rs`（4 例，跨 http × core × crypto-engine
//! 三 crate）与 `bedcode-server-base/tests/error_envelope_ipc.rs`（3 例，`AppError` ×
//! Tauri IPC），已迁至 `src-tauri/tests/`，两个 crate 的 dev 内部依赖同步删除。
//!
//! ## 覆盖面按**目录约定**推导，不靠手写名单
//!
//! 治理面 = `bedcode-desktop/packages/` 下每个 `bedcode-*` 目录，**新增 crate 自动纳入**。
//! 这是 `empty_dir_lock.rs` 的同一手法：手写名单天然是一条零成本后门（删掉一行覆盖面
//! 少一块而锁照绿）。
//!
//! ## 唯一的登记例外桶
//!
//! - `bedcode-wasm-core`：机制整核，另有一条在途会话正在改它，本轮明确不扫（用户裁定）。
//!   待其票据完成后删掉本条目即自动进入本锁管辖。进本桶不是免费的——条目自带理由文本，
//!   理由不写清楚就会变成垃圾桶。
//!
//! ## 明确不在本锁宇宙内（且**不是**遗漏）
//!
//! - `bedcode-host-kit`（仓库根 `packages/`，不在 `bedcode-desktop/packages/` 下，与
//!   `cross-end-tests` 等共享）：它的 `tests/forced_link.rs` + `tests/forced_link_absent.rs`
//!   是**按设计**的跨 crate 集成测试（探针 fixture crate + 链接器二进制两个 target 钉「强制
//!   引用行漏掉 ⇒ 注册丢失」两侧），搬迁要连 fixture 一起搬，另立票据。
//! - `plugin-sdk-*` / `plugin-*-test` / `plugin-sdk-fixtures`：契约与夹具 crate，按设计
//!   非 `bedcode-` 前缀。
//! - `bedcode-server-base/src/` 内的 `#[cfg(test)]` 单元测试模块：**本锁要的就是它们**，
//!   不在禁列。

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

// ==================== 登记表与扫描面 ====================

/// 已知但本轮**不**纳入管辖的 crate（须写明在办票据；处置完删条目即自动纳入）
///
/// 与目录推导出的治理面合起来构成完整覆盖：治理面 = `packages/bedcode-*` 减去本桶。
/// 没有本桶，「把 crate 排除出管辖」就是一条零成本后门。
const PENDING_GOVERNANCE: &[(&str, &str)] = &[(
    "bedcode-wasm-core",
    "机制整核由独立在途会话改造中（用户裁定本轮不扫）；票据完成后删本条目即自动纳入",
)];

/// crate 根下**禁止**存在的集成测试面目录（cargo 自动发现即编译成独立测试二进制）
///
/// `benches` / `examples` 一并列禁：它们同样是「只能经 pub API 访问对外行为」的编译面，
/// 且在引擎 crate 里同样会诱发 dev 依赖追加。
const FORBIDDEN_CRATE_TEST_DIRS: &[&str] = &["tests", "benches", "examples"];

/// 内部 crate 的命名前缀（dev 段里出现即为一条只存在于测试的内部依赖边）
const INTERNAL_CRATE_PREFIX: &str = "bedcode";

/// 迁移到宿主侧的集成测试证据（crate 内文件 + 该文件必须保留的用例数）
///
/// **存在性钉死**：C-2/C-3 只禁「crate 里出现集成测试」，禁不掉「有人把越线的测试直接
/// 删掉」——那会让锁全绿而覆盖面静默消失。故正面钉住：文件必须在宿主侧，且用例数不得
/// 减少（减少即测红，改用例必须回本锁说明）。
struct MigratedEvidence {
    /// 相对 `src-tauri` 的路径
    host_rel: &'static str,
    /// 从哪个 crate 的 `tests/` 迁来（报错文案用）
    came_from: &'static str,
    /// 该文件里必须保留的测试用例数（`#[test]` 与 `#[actix_web::test]` 合计）
    min_cases: usize,
}

const MIGRATED_EVIDENCE: &[MigratedEvidence] = &[
    MigratedEvidence {
        host_rel: "tests/link_crypto_http.rs",
        came_from: "packages/bedcode-server-http/tests/",
        min_cases: 4,
    },
    MigratedEvidence {
        host_rel: "tests/error_envelope_ipc.rs",
        came_from: "packages/bedcode-server-base/tests/",
        min_cases: 3,
    },
];

// ==================== 路径工具 ====================

/// 宿主根目录（`bedcode-desktop/src-tauri`）
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `bedcode-desktop/packages/`
fn packages_dir() -> PathBuf {
    crate_root()
        .parent()
        .expect("src-tauri 的上级应是 bedcode-desktop")
        .join("packages")
}

/// 推导治理面：`packages/` 下每个 `bedcode-*` 目录，减去 `PENDING_GOVERNANCE`
fn governed_crates() -> Vec<String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(packages_dir())
        .unwrap_or_else(|e| panic!("读不到 {} —— 扫描器空转，本锁失效：{e}", packages_dir().display()));
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(INTERNAL_CRATE_PREFIX) || !entry.path().is_dir() {
            continue;
        }
        if PENDING_GOVERNANCE.iter().any(|(n, _)| *n == name) {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

fn read_manifest(crate_name: &str) -> String {
    let path = packages_dir().join(crate_name).join("Cargo.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {} —— 扫描器空转，本锁失效：{e}", path.display()))
}

// ==================== 清单解析 ====================

/// 解析行首的 TOML 段头 → `(段种类, 子键)`
///
/// 覆盖三种段头形态（不识别的直接返回 `None`，由调用方按「普通行」处理）：
/// - `[dependencies]` → `("dependencies", None)`
/// - `[dependencies.foo]` / `[dev-dependencies.bedcode-x]` → `("dependencies", Some("foo"))`
///   ——**点号表形式**，`cargo` 全面支持，是 dev 内部依赖最常见的藏身处（首版解析只看
///   `[dev-dependencies]` 段内条目名，整条边从锁的视野里消失；由变异注入打红）
/// - `[[test]]` / `[[bin]]` 等数组表 → `("test", None)` / `("bin", None)`
fn section_header(line: &str) -> Option<(String, Option<String>)> {
    if !line.starts_with('[') || !line.ends_with(']') {
        return None;
    }
    let inner = line[1..line.len() - 1].trim();
    // `[[…]]`：剥掉两层方括号
    let inner = if inner.starts_with('[') && inner.ends_with(']') {
        inner[1..inner.len() - 1].trim()
    } else {
        inner
    };
    Some(match inner.split_once('.') {
        Some((kind, sub)) => (kind.to_string(), Some(sub.to_string())),
        None => (inner.to_string(), None),
    })
}

/// 取清单某个依赖段里的条目名集合（如 `[dependencies]` → `{"serde", "tokio", …}`）
///
/// **按行切段**，不按下一个 `[` 找段尾：段内值常含 `features = ["derive"]` 这类方括号，
/// 那种切法会把清单截断在 serde 行上（`bedcode-server-websocket` 的
/// `dependency_direction_lock` 首版实测踩中，表现为误报「清单缺少内核依赖」）。
///
/// **不支持的形态一律 panic，不静默跳过**（本锁宁可红不可绿）：
/// - `[target.'cfg(...)'.dependencies]` 段头——按行切段会把它当成新段头而**静默丢掉**
///   其下的依赖，那正是会让 dev 内部依赖逃过本锁的形态；
/// - 条目行花括号不配平（依赖声明被折行）——续行会被当第二个条目名静默收进集合；
/// - 条目名不是 `[A-Za-z0-9_.-]+`——其余未覆盖写法一律报错，不猜。
fn section_deps(manifest: &str, section: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_section = false;
    for (idx, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if let Some((kind, sub)) = section_header(t) {
            // `[target.…]` 段头**无条件**报错：它按行切段会被当成新段头，其下的依赖随之
            // 静默丢弃，而那正是会让 dev 内部依赖逃过本锁的形态。注意判据不能挂在
            // `in_section` 上——首版就挂错了：`[target.…]` 若出现在 `[dependencies]`
            // 之前则永远不触发守卫（自检用例 C-1 亲手把它打红）。
            if kind == "target" || kind.starts_with("target.") {
                panic!(
                    "第 {} 行出现 `[target.…]` 段头（`{t}`）：本锁的按行切段解析不支持它，\
                     会静默丢掉该段下的依赖——请先把本解析升级到能处理该形态，而不是让锁失守",
                    idx + 1
                );
            }
            if kind == section {
                match sub {
                    // 点号表形式：`[dev-dependencies.bedcode-x]` 就是一条 dev 内部依赖
                    Some(dep_name) => {
                        out.insert(dep_name);
                        in_section = false;
                    }
                    None => in_section = true,
                }
            } else {
                in_section = false;
            }
            continue;
        }
        if !in_section || t.is_empty() || t.starts_with('#') {
            continue;
        }
        // **跨行依赖声明**守卫：条目行的花括号必须自身配平。不配平就是声明被折行（如
        // `serde = { version = "1",` 换行写 features），而本解析是按行取 `=` 左边的
        // 条目名——续行会被当成第二个条目名（`features`）静默收进集合，看着「解析成功」。
        // 首版只校验条目名合法性，恰好挡不住这种形态（续行 `features` 也是合法标识符），
        // 由 C-1 的多行夹具打红。
        assert_eq!(
            t.matches('{').count(),
            t.matches('}').count(),
            "第 {} 行 `{t}` 花括号不配平：依赖声明被折行，本锁的按行解析会把它误收成第二个\
             条目名而静默失守。折行写法请先改成单行，或升级本解析后再移开门禁",
            idx + 1
        );
        let key = t.split('=').next().unwrap_or("").trim().to_string();
        assert!(
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'),
            "第 {} 行 `{t}` 不是可解析的依赖条目名——多行依赖声明等未覆盖写法出现时必须报错，\
             不能猜着解析（否则会静默漏判）",
            idx + 1
        );
        out.insert(key);
    }
    out
}

/// 清单里显式登记的 `[[test]]` / `[[bench]]` / `[[example]]` 段名
fn declared_extra_targets(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in manifest.lines() {
        let t = line.trim();
        for kind in ["test", "bench", "example"] {
            if t == format!("[[{kind}]]") {
                out.push(format!("[[{kind}]]"));
            }
        }
    }
    out
}

// ==================== 用例 ====================

/// C-1｜扫描器防空转：解析器必须真能认出条目，且不被段内方括号 / 段边界骗过
///
/// 锁最危险的失效形态是「扫描器悄悄什么都不匹配」，那种锁永远绿。四条各自可杀的证明：
/// 段内方括号不截断清单、dev 段与 prod 段分别解析、`[build-dependencies]` 不污染、
/// 未覆盖写法必须 panic 而不是静默。
#[test]
fn scanner_is_not_vacuous() {
    let manifest = "\
[package]
name = \"x\"

[dependencies]
serde = { version = \"1\", features = [\"derive\"] }
bedcode-server-core = { path = \"../bedcode-server-core\" }

[dev-dependencies]
bedcode-crypto-engine = { path = \"../bedcode-crypto-engine\" }

[build-dependencies]
bedcode-server-http = \"1\"
";
    let prod = section_deps(manifest, "dependencies");
    let dev = section_deps(manifest, "dev-dependencies");
    let build = section_deps(manifest, "build-dependencies");

    // 段内方括号不得截断清单（否则 serde 行之后的依赖全丢，误报「清单为空」）
    assert!(
        prod.contains("bedcode-server-core"),
        "段内 `features = [\"derive\"]` 截断了依赖清单，实得 {prod:?}"
    );
    assert!(prod.contains("serde"), "行内方括号条目必须被解析，实得 {prod:?}");

    // prod / dev 分别解析，且互不串段
    assert!(
        !prod.contains("bedcode-crypto-engine"),
        "`[dev-dependencies]` 的条目不得进 prod 集合，实得 {prod:?}"
    );
    assert!(
        dev.contains("bedcode-crypto-engine") && !dev.contains("bedcode-server-core"),
        "dev 段解析错误，实得 {dev:?}"
    );
    // build 段是第三段，不得被当成 dev
    assert!(
        build.contains("bedcode-server-http") && !dev.contains("bedcode-server-http"),
        "`[build-dependencies]` 不得被并入 dev 段（否则会误报一条不存在的 dev 内部依赖），实得 dev={dev:?} build={build:?}"
    );

    // 点号表形式（`[dev-dependencies.bedcode-x]` / `[dependencies.foo]`）必须被认出并
    // 归到**正确的段**——这是 dev 内部依赖最常见的藏身处：首版解析只认 `[dev-dependencies]`
    // 段内的条目名，整条边从锁视野里消失（由变异注入 `[dev-dependencies.bedcode-crypto-engine]`
    // 打红，本条防复发）。
    let dotted = "\
[dependencies]
bedcode-server-core = \"1\"

[dev-dependencies.bedcode-crypto-engine]
path = \"../bedcode-crypto-engine\"

[dependencies.another-crate]
version = \"1\"
";
    let d_prod = section_deps(dotted, "dependencies");
    let d_dev = section_deps(dotted, "dev-dependencies");
    assert!(
        d_dev.contains("bedcode-crypto-engine"),
        "点号表 `[dev-dependencies.bedcode-crypto-engine]` 必须被解析成一条 dev 内部依赖，实得 {d_dev:?}"
    );
    assert!(
        !d_prod.contains("bedcode-crypto-engine"),
        "dev 段的点号表不得被算进 prod 段（否则 dev 内部边会被当成生产依赖而放行），实得 {d_prod:?}"
    );
    assert!(
        d_prod.contains("another-crate") && d_prod.contains("bedcode-server-core"),
        "`[dependencies.foo]` 点号表必须被解析成 prod 条目，实得 {d_prod:?}"
    );
    assert!(
        !d_dev.contains("another-crate"),
        "prod 段的点号表不得被算进 dev 段，实得 {d_dev:?}"
    );

    // 未覆盖写法必须 panic（fail-visible），不是静默跳过
    let unsupported = "[target.'cfg(unix)'.dependencies]\nbedcode-server-core = \"1\"\n";
    let panicked = std::panic::catch_unwind(|| section_deps(unsupported, "dependencies"));
    assert!(
        panicked.is_err(),
        "出现 `[target.…]` 段头时必须 panic（按行切段会静默丢掉该段依赖）——不 panic 即锁失守"
    );
    let malformed = "[dependencies]\nserde = { version = \"1\",\nfeatures = [\"derive\"] }\n";
    let panicked2 = std::panic::catch_unwind(|| section_deps(malformed, "dependencies"));
    assert!(panicked2.is_err(), "多行依赖声明等未覆盖写法必须 panic——继续解析等于猜");

    // 目录发现逻辑：空目录下的推导结果必须为空（不得凭空造出 crate 名）
    let dir = tempfile::tempdir().expect("临时目录");
    fs::create_dir_all(dir.path().join("bedcode-not-really-here")).expect("建假 crate 目录");
    let found: Vec<String> = fs::read_dir(dir.path())
        .expect("读临时目录")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(INTERNAL_CRATE_PREFIX) && dir.path().join(n).is_dir())
        .collect();
    assert_eq!(
        found,
        vec!["bedcode-not-really-here".to_string()],
        "目录推导必须只认 `bedcode-` 前缀的真目录"
    );
}

/// C-2｜主判据①：治理 crate 不得有 crate 根集成测试面（目录 + `[[test]]` 段两个形态）
#[test]
fn governed_crates_have_no_crate_level_integration_test_surfaces() {
    let crates = governed_crates();
    assert!(!crates.is_empty(), "推导出的治理面为空 ⇒ 扫描器空转，本锁必须失效报错");

    let mut violations: Vec<String> = Vec::new();
    for crate_name in &crates {
        let root = packages_dir().join(crate_name);
        for dir in FORBIDDEN_CRATE_TEST_DIRS {
            let path = root.join(dir);
            if path.is_dir() {
                let mut entries: Vec<String> = fs::read_dir(&path)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                entries.sort();
                violations.push(format!(
                    "{crate_name}/{dir}/ 存在（{}）—— crate 根的 `{dir}/` 是只能经 pub API \
                     访问对外行为的可编译面。跨 crate 集成测试请迁到宿主 `src-tauri/tests/`",
                    entries.join(", ")
                ));
            }
        }
        let declared = declared_extra_targets(&read_manifest(crate_name));
        if !declared.is_empty() {
            violations.push(format!(
                "{crate_name}/Cargo.toml 登记了 {}—— 与 crate 根测试面同一形态，治理 crate 不登记",
                declared.join(", ")
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "拆分产物 crate 只保留单元测试（`src/` 内的 `#[cfg(test)]`），跨 crate 集成测试归宿主：\n  - {}",
        violations.join("\n  - ")
    );
}

/// C-3｜主判据②：治理 crate 的 `[dev-dependencies]` 不得含任何内部 crate
///
/// 只服务 `src/` 内单元测试夹具的外部 dev 依赖（`tempfile` / `tracing-subscriber` 等）
/// 不在此列——本条禁的是**依赖图里多出一条只为测试存在的内部边**。
#[test]
fn governed_crates_have_no_dev_only_internal_crate_dependencies() {
    let crates = governed_crates();
    assert!(!crates.is_empty(), "推导出的治理面为空 ⇒ 扫描器空转，本锁必须失效报错");

    let mut violations: Vec<String> = Vec::new();
    for crate_name in &crates {
        let manifest = read_manifest(crate_name);
        let prod = section_deps(&manifest, "dependencies");
        let dev = section_deps(&manifest, "dev-dependencies");
        for dep in &dev {
            if !dep.starts_with(INTERNAL_CRATE_PREFIX) {
                continue;
            }
            // 同名出现在 prod 段 = 正常生产依赖（dev 段重复声明只调 feature），不违规
            if prod.contains(dep) {
                continue;
            }
            violations.push(format!(
                "{crate_name}/Cargo.toml 的 [dev-dependencies] 含内部 crate `{dep}`\
                 （生产清单里没有它 ⇒ 只为测试存在的一条依赖边）。跨 crate 组合测试请迁到宿主 \
                 `src-tauri/tests/`，那里全部依赖都是生产依赖"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "拆分产物 crate 不得有只服务于测试的内部依赖边：\n  - {}",
        violations.join("\n  - ")
    );
}

/// C-4｜覆盖面：推导面非空、待扫描桶不得留陈旧条目、治理 crate 真实可扫
///
/// 防两类退化：① 目录推导意外落空（路径改名 / 层级变动 ⇒ 覆盖面静默归零而锁照绿）；
/// ② `PENDING_GOVERNANCE` 变成垃圾桶（条目不再需要却留着 = 覆盖面静默少一块）。
#[test]
fn governance_coverage_is_derived_and_pending_bucket_cannot_rot() {
    let discovered: Vec<String> = fs::read_dir(packages_dir())
        .unwrap_or_else(|e| panic!("读不到 {}：{e}", packages_dir().display()))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(INTERNAL_CRATE_PREFIX) && packages_dir().join(n).is_dir())
        .collect();

    assert!(
        !discovered.is_empty(),
        "`packages/` 下没有任何 `bedcode-*` 目录 —— 目录推导落空，本锁必须失效报错"
    );
    for (name, reason) in PENDING_GOVERNANCE {
        assert!(
            discovered.iter().any(|n| n == name),
            "待扫描桶登记的 `{name}` 在磁盘上已不存在 —— 删掉本条目即恢复管辖（理由：{reason}）"
        );
        assert!(
            !reason.trim().is_empty(),
            "待扫描条目 `{name}` 必须写明在办票据，理由不得留空（空理由 = 垃圾桶入口）"
        );
    }

    for crate_name in governed_crates() {
        let manifest_path = packages_dir().join(&crate_name).join("Cargo.toml");
        assert!(
            manifest_path.is_file(),
            "`{crate_name}` 没有 Cargo.toml —— 清单解析会 panic，本锁对该 crate 失效"
        );
        let src = packages_dir().join(&crate_name).join("src");
        assert!(
            src.is_dir(),
            "`{crate_name}` 没有 src/ 目录 —— crate 被搬走或改名，必须同改本锁的推导规则"
        );
        let _ = read_manifest(&crate_name);
    }
}

/// C-5｜迁移证据不得静默消失：文件在宿主侧，且用例数不得减少
///
/// C-2/C-3 只能禁「crate 里出现集成测试」，禁不掉「把越线的测试直接删掉」——那会让锁全绿
/// 而覆盖面悄悄消失。故正面钉住宿主侧的落点与用例数。
#[test]
fn migrated_integration_tests_still_live_on_host_side() {
    assert!(
        !MIGRATED_EVIDENCE.is_empty(),
        "迁移证据表为空 ⇒ 本锁的「不得删测试」侧失效，必须立即报错"
    );
    let mut violations: Vec<String> = Vec::new();
    for ev in MIGRATED_EVIDENCE {
        let path = crate_root().join(ev.host_rel);
        let Ok(text) = fs::read_to_string(&path) else {
            violations.push(format!(
                "{} 不存在（自 {} 迁来）—— 锁只禁「集成测试住在 crate 里」，禁不掉「直接删掉」，\
                 故此处正面钉住落点",
                ev.host_rel, ev.came_from
            ));
            continue;
        };
        let cases = text.matches("#[test]").count() + text.matches("#[actix_web::test]").count();
        if cases < ev.min_cases {
            violations.push(format!(
                "{} 只剩 {cases} 个用例（登记下限 {}）—— 覆盖面静默缩水。改动用例必须回本锁说明理由",
                ev.host_rel, ev.min_cases
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "已迁移的集成测试证据缺失：\n  - {}",
        violations.join("\n  - ")
    );
}
