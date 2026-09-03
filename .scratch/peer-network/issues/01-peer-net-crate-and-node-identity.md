# 01 — peer-net crate 骨架 + 节点身份

**What to build:** 建成共享 crate `bedcode-peer-net` 并被桌面端与移动端 src-tauri 以 path 依赖引入；每个 App 实例首次启动生成长效 Ed25519 节点身份——节点 ID 即公钥指纹、密钥跨重启稳定、重装即新身份；由该密钥经 rcgen 产出自签 TLS 证书。双节点测试 harness（进程内两实例、回环端口互指）随票建立，作为后续所有无头行为的主验证缝。

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [x] 两端可编译并引入 crate；`cargo test` 通过
- [x] harness 中两个节点各持身份：重启（重新加载持久化密钥）后节点 ID 不变
- [x] 自签证书可生成且公钥与节点 ID 一致；证书绑定校验有单测
- [x] 身份持久化遵循两端宿主现有的本地存储惯例

## Comments

2026-08-22 实现（worker）：

- 新增共享 crate `bedcode-peer-net`（仓库根 `packages/peer-net`）：`error.rs`（thiserror 统一错误）、`identity.rs`（NodeId/NodeIdentity/load_or_create 原子写 + unix 0600）、`cert.rs`（rcgen 自签证书 + SPKI 绑定校验）、`node.rs`（PeerNetNodeConfig/StaticPeerRecord 形状缝）、`tests/harness.rs`（双节点回环互指 harness）。决策 D1–D5 全部落地：身份首启 OsRng 纯随机（D2）、损坏文件快速失败不静默重建（D3）、rcgen 密钥由身份种子经 RFC 8410 固定 PKCS#8 前缀构造（D4 关键点，单测锁死「证书公钥 == 身份公钥」及伪造拒止）、配置形状定型不监听（D5）。
- 两端接线：`Cargo.toml` 各加一行 path 依赖；各自新增薄模块 `src/peer_net.rs`（`init_node_identity` 打印长 ID + 短指纹），桌面端 setup 在 DB 同源 app_data_dir 解析点调用，移动端复用 `init_identity` 的 app_data_dir 解析点调用。未加 Tauri command（计划内，ticket 02 再开放）。
- 测试：crate `cargo test` 13 单测 + 3 集成全绿；mobile `cargo test` 393 lib + 集成/doc 共 415 passed / 0 failed；desktop `cargo test` 576 lib + 6 个集成目标共 582 passed / 0 failed。
- 偏差与备注：
  - 两端 `src-tauri/Cargo.lock` 被 `.gitignore`（`**/src-tauri/**/Cargo.lock`）忽略、本就不入库，无变更可提交；`packages/peer-net/Cargo.lock` 已生成，与 plugin-sdk 先例一致应入库（未 commit，待主流程统一提交）。
  - 桌面端验证期间遭遇环境级构建干扰：火绒 HipsDaemon 实时扫描与 cargo 并行写入竞争，导致链接期随机出现 E0460/E0462/E0463（失败 crate 每次不同：petgraph/windows_sys/flate2/actix_web/cookie…）。`cargo build/test -j 1` 串行构建稳定通过；建议将项目目录加入火绒白名单，或在受影响机器上以 `-j` 低并行构建。
  - 排查过程中对 desktop `target` 执行过一次 `cargo clean`（当时增量元数据已损坏），下次桌面端首次构建为全量重编，耗时较长属预期。
