//! 宿主侧 WIT 绑定（全 world `bindgen!`，路径 B 能力域共用一份）
//!
//! **为什么宿主要自带一份绑定**（孤儿规则，票 02 §4「宿主端组合形态」）：
//! `impl Trait for WasmPluginState` 要求 trait 或类型之一本地——`WasmPluginState`
//! 属 host-kit（对任何消费方都是外部类型），故**装配方必须自己生成 trait**。
//! 内核（`manager/runtime/component.rs`）与各能力 crate（provider 侧）各自 bindgen
//! 同一份 WIT，三处生成的 `bedcode::plugin::host_*::Host` **同名但不同类型**。
//!
//! ## 纪律（批次 04/05 的路径 B 域）
//!
//! - 宿主侧路径 B 域（批次 04 = crypto / auth；批次 05 = task / process / app /
//!   timer / api_call / connection）的 `Host` impl 一律引用**本文件**的绑定；
//! - **不得**再实现内核或能力 crate 生成的同名 trait——同一个 interface 被两份
//!   `add_to_linker` 注册 ⇒ 装配期 `defined twice`（`bedcode-server-http`
//!   plugin_binding 的同名 trait 警示同源）；
//! - 绑定配置与内核逐字一致（`exports: { default: async }`——票 02 宿主 async 化
//!   门禁：wasip3 async store 下同步 call 报错，统一 async 化让 wasip2/unknown
//!   插件与 wasip3 走同一条调用路径）。

use wasmtime::component::bindgen;

bindgen!({
    // 票 03：端 `wit/` 是生成物目录（同 package 拼装，push_dir 加载）
    path: "../packages/plugin-sdk-desktop/rust/wit",
    world: "plugin",
    exports: { default: async },
});

#[cfg(test)]
mod tests {
    //! 票 03 §4.4：端清单（compose.json）↔ 宿主装配面静态对照锁
    //!
    //! 单一事实源口径（spec §2）：一份 `<end>-capabilities.json` 同时驱动 ① WIT
    //! 拼装 ② 宿主白名单 ③ ABI 计数——「组合了什么」只有一个答案。本测试钉住
    //! 桌面清单的字段完整性与它声明的组合面，漂移（增删组合 / 改 ABI / 改
    //! world）即红。
    //!
    //! 对照口径（首步静态版，语义核对留票 04/07 加深）：
    //! - caps 键 = 组合单元：`pty` / `http` / `ws` 对应能力域引擎 crate 的
    //!   MODULE_NAME（pty / http / websocket，宿主 Cargo.toml 依赖即编译期证明），
    //!   `desktop` 对应宿主 `src/plugin/` 路径 B 域（MODULE_INTERFACES 并集）；
    //! - cap-desktop.wit 的 world cap-desktop import 名集合 == 宿主路径 B 域
    //!   MODULE_INTERFACES 的 host-* 名并集（12 个）。
    //! - abi.version 同时是票 04 的 ABI 计数输入（SDK abi.rs 的 ABI_VERSION）。

    use std::path::PathBuf;

    fn manifest_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../packages/plugin-sdk-desktop/compose.json")
    }

    /// 从 WIT 文本提取顶层 import 声明名（`import <name>;`）
    fn wit_imports(text: &str) -> Vec<&str> {
        text.lines()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("import ") && l.ends_with(';'))
            .map(|l| l.trim_start_matches("import ").trim_end_matches(';').trim())
            .collect()
    }

    #[test]
    fn compose_manifest_fields_are_complete() {
        let raw = std::fs::read_to_string(manifest_path())
            .unwrap_or_else(|e| panic!("端清单缺失: {e}"));
        let m: serde_json::Value = serde_json::from_str(&raw)
            .expect("端清单 JSON 可解析（compose-wit.mjs 的消费前提）");

        assert_eq!(m["world"], "plugin", "主 world 名是组合装配面");
        assert_eq!(m["package"], "package bedcode:plugin;", "同 package 拼装（裁决 A）");

        let mut caps: Vec<&str> = m["caps"].as_object().unwrap().keys().map(|s| s.as_str()).collect();
        caps.sort();
        assert_eq!(caps, vec!["desktop", "http", "pty", "ws"],
            "caps 键漂移：能力域 pty/http/ws（引擎 crate）+ desktop（宿主路径 B 域聚合）是当前唯一答案");

        for key in ["pty", "http", "ws", "desktop"] {
            let src = m["caps"][key].as_str().expect("caps 值为字符串路径");
            let abs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join(src);
            assert!(abs.exists(), "cap 真源缺失: {src}");
        }

        let mut worlds: Vec<&str> = m["worlds"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        worlds.sort();
        assert_eq!(worlds, vec!["plugin", "plugin-auth-policy", "plugin-binary",
            "plugin-system", "plugin-task", "plugin-ws"],
            "worlds 漂移：桌面 6 个 world 是合成 package 的完整面");

        assert_eq!(m["abi"]["version"], 37, "abi.version 漂移：与 SDK abi.rs ABI_VERSION 对齐（票 04 交集切片收拢）");
    }

    #[test]
    fn cap_desktop_imports_match_host_path_b_modules() {
        let cap = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../packages/plugin-sdk-desktop/rust/wit/cap-desktop.wit"),
        ).unwrap_or_else(|e| panic!("cap-desktop.wit 缺失: {e}"));

        let world_body = cap.split("world cap-desktop {").nth(1)
            .expect("cap-desktop.wit 含 world cap-desktop")
            .split('}').next().unwrap();
        let mut imports = wit_imports(world_body);
        imports.sort();
        imports.dedup();

        // 宿主路径 B 域 MODULE_INTERFACES 并集（11 个 host-* 名）；能力域
        // （pty/http/websocket/peer/mdns）interface 在各自 cap-*.wit，不在本文件。
        // 票 04：`host-fs` 交集 6 收拢进共享核心 core.wit，cap-desktop 不再 import 它。
        let expected = [
            "host-api-call", "host-app", "host-auth", "host-connection", "host-crypto",
            "host-events-desktop", "host-fs-desktop", "host-platform-desktop",
            "host-process", "host-task", "host-timer",
        ];
        assert_eq!(imports, expected,
            "cap-desktop.wit 的 import 面漂移：与宿主路径 B 域 MODULE_INTERFACES 并集不一致");
    }

    // ==================== 票 07 批次 02：组合面 / 装配面动态锁 ====================

    /// 收集 WIT 源码里的 interface 定义名（`interface <name> {`）
    fn collect_interface_defs(src: &str, out: &mut Vec<String>) {
        for line in src.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("interface ") {
                if let Some(name) = rest.split_whitespace().next() {
                    out.push(name.to_string());
                }
            }
        }
    }

    /// 提取 `world <name> { … }` 体内的 import interface 名（`import <name>;`）
    fn wit_world_imports(src: &str, world: &str) -> Vec<String> {
        let marker = format!("world {world} {{");
        let Some(pos) = src.find(&marker) else {
            return Vec::new();
        };
        let body = &src[pos + marker.len()..];
        let end = body.find('}').unwrap_or(body.len());
        body[..end]
            .lines()
            .filter_map(|l| {
                let t = l.trim();
                t.strip_prefix("import ")
                    .and_then(|rest| rest.strip_suffix(';'))
                    .map(|name| name.trim().to_string())
            })
            .collect()
    }

    /// 能力域 crate 自持分片 bindgen 锁（票 05 终态；票 07 批次 02 收口）
    ///
    /// 五能力域 crate 的 provider 侧 `bindgen!` 必须指向**自家分片**
    /// （`path: "wit/<domain>.wit"` + `world: "cap-<domain>"`）。回接形态 = path 又指
    /// 回端生成物目录（`plugin-sdk-desktop/rust/wit`）或 world 用整份 `plugin`——
    /// 那是票 01 POC 实测的第一性问题：能力域在契约面绑死端，移动侧无法复用同一
    /// 能力域 crate，「差异只来自组合的能力 lib」不成立。
    ///
    /// 判据按**代码行**比对（跳注释行）：这些文件的模块文档正当地提到迁移史。
    #[test]
    fn capability_domains_bind_their_own_wit_slices() {
        // (crate 目录, 绑定源文件, 自持分片, world)
        const DOMAINS: &[(&str, &str, &str, &str)] = &[
            ("packages/bedcode-pty-engine", "src/plugin_binding.rs", "wit/pty.wit", "cap-pty"),
            ("packages/bedcode-server-http", "src/plugin_binding.rs", "wit/http.wit", "cap-http"),
            (
                "packages/bedcode-server-websocket",
                "src/plugin_binding.rs",
                "wit/ws.wit",
                "cap-ws",
            ),
            ("packages/bedcode-server-peer-net", "src/plugin_binding.rs", "wit/peer.wit", "cap-peer"),
            ("packages/bedcode-discovery-engine", "src/lib.rs", "wit/mdns.wit", "cap-mdns"),
        ];
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (crate_dir, src_rel, slice_rel, world) in DOMAINS {
            let src_path = repo_root.join(crate_dir).join(src_rel);
            let source = std::fs::read_to_string(&src_path)
                .unwrap_or_else(|e| panic!("不可读 {}: {e}", src_path.display()));
            // 分片真源本体在场（path 指向不存在的文件同样要红）+ world 声明在分片内
            let slice_path = repo_root.join(crate_dir).join(slice_rel);
            let slice_src = std::fs::read_to_string(&slice_path)
                .unwrap_or_else(|e| panic!("自持分片缺失 {}: {e}", slice_path.display()));
            assert!(
                slice_src
                    .lines()
                    .any(|l| l.trim_start().starts_with(&format!("world {world}"))),
                "{crate_dir}/{slice_rel} 未声明 `world {world}`（path/world 至少一侧指错）"
            );

            let code_lines: Vec<&str> = source
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect();
            assert!(
                code_lines.iter().any(|l| l.contains(&format!("path: \"{slice_rel}\""))),
                "{crate_dir}/{src_rel} 的 bindgen 未指向自持分片 `path: \"{slice_rel}\"`\
                 （票 05 终态：能力域不得再绑端 WIT）"
            );
            assert!(
                code_lines.iter().any(|l| l.contains(&format!("world: \"{world}\""))),
                "{crate_dir}/{src_rel} 的 bindgen 未声明 `world: \"{world}\"`"
            );
            for banned in ["plugin-sdk-desktop", "plugin-sdk-mobile"] {
                if let Some(hit) = code_lines.iter().find(|l| l.contains(banned)) {
                    panic!(
                        "{crate_dir}/{src_rel} 代码行出现端 SDK WIT 引用 `{banned}`（行：{hit}）——\
                         能力域在契约面绑死端（票 01 POC 第一性问题），回接即移动侧无法复用本 crate"
                    );
                }
            }
        }
    }

    /// 端清单 ↔ 宿主装配面**动态**对照锁（票 07 批次 02：票 03 §4.4 静态版的加深）
    ///
    /// 静态版（`compose_manifest_fields_are_complete` /
    /// `cap_desktop_imports_match_host_path_b_modules`）钉住 caps 键集与真源在场；
    /// 本锁把它推到语义面——用两侧共有的词汇（interface 路径）做双向覆盖，即票面
    /// 「清单域名集 == 白名单能力域模块集」的可执行形态：
    ///
    /// ① 正向（cap world import → 模块）：每个 cap 世界 import 的 interface 都必须有
    ///    白名单模块认领（组合进了插件 import 集却没人装配 ⇒ 实例化即炸）；
    /// ② 反向（模块 interface → 合成 package）：每个白名单模块声明的每个 interface
    ///    都必须在 core.wit 或某一 cap 真源里有定义（装配面与组合面脱节 ⇒ 红）。
    ///
    /// 覆盖端 = 桌面（白名单机制在宿主侧；移动端无 host-kit 注册表，对照面在
    /// 移动 compose.json 与移动 WIT 生成物的等价性，由漂移锁 + 移动宿主测试各自兜住）。
    #[test]
    fn compose_caps_and_host_module_interfaces_cover_each_other() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        // ① 装配面：白名单模块 → interface 名（`bedcode:plugin/host-x` → `host-x`）
        let registry = bedcode_host_kit::ModuleRegistry::collected();
        let descs = registry.descs();
        assert!(
            descs.len() >= 10,
            "能力模块收集面过小（{} 个），锁可能空转（能力 crate 未链接 / 编译面缺 desktop-host）",
            descs.len()
        );
        let mut module_ifaces: Vec<(String, String)> = Vec::new();
        for d in &descs {
            for iface in d.interfaces {
                module_ifaces.push((
                    iface.rsplit('/').next().unwrap_or(iface).to_string(),
                    d.name.to_string(),
                ));
            }
        }
        // ② 组合面：caps 真源 + core.wit
        let raw =
            std::fs::read_to_string(manifest_path()).unwrap_or_else(|e| panic!("端清单缺失: {e}"));
        let m: serde_json::Value = serde_json::from_str(&raw).expect("端清单 JSON 可解析");
        let caps = m["caps"].as_object().expect("caps 是对象");
        let mut cap_worlds: Vec<(String, String)> = Vec::new(); // (cap key, 真源内容)
        for (key, v) in caps {
            let path = repo_root.join(v.as_str().expect("cap 值为路径"));
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cap 真源不可读 {}: {e}", path.display()));
            cap_worlds.push((key.clone(), src));
        }
        let mut defined: Vec<String> = Vec::new();
        {
            let core_path = repo_root.join(m["core"].as_str().expect("core 为路径"));
            let core_src = std::fs::read_to_string(&core_path)
                .unwrap_or_else(|e| panic!("core.wit 不可读 {}: {e}", core_path.display()));
            collect_interface_defs(&core_src, &mut defined);
        }
        for (_, src) in &cap_worlds {
            collect_interface_defs(src, &mut defined);
        }
        // ① 正向：cap 世界组合的 interface 必须有人装配
        for (key, src) in &cap_worlds {
            let world = format!("cap-{key}");
            let imports = wit_world_imports(src, &world);
            assert!(
                !imports.is_empty(),
                "cap `{key}` 的 `world {world}` 无 import（判据会空转：真源文件或 world 名写错？）"
            );
            for iface in &imports {
                let owners: Vec<&str> = module_ifaces
                    .iter()
                    .filter(|(n, _)| n == iface)
                    .map(|(_, module)| module.as_str())
                    .collect();
                assert!(
                    !owners.is_empty(),
                    "cap `{key}` 组合了 interface `{iface}`，但没有任何白名单模块认领它——\
                     组合面与装配面脱节（补能力模块自报 + expect_host_module!，或从 caps 摘除）"
                );
            }
        }
        // ② 反向：装配的 interface 必须在合成 package 里有定义
        for (iface, module) in &module_ifaces {
            assert!(
                defined.contains(iface),
                "白名单模块 `{module}` 声明 interface `{iface}`，但它不在合成 package\
                 （core.wit ∪ caps）里定义——装配面与组合面脱节"
            );
        }
    }
}
