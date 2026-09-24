# 08 — 收尾：全量回归 + 文档记账

**Type:** task
**Blocked by:** 01, 02, 03, 04, 05, 06, 07
**Status:** ready-for-agent

**What to build:** 本轮解耦重构的收尾验证与文档同步。所有前置票完成且单项测试满绿后：

- **全量回归（§3 黄金命令，必须实际运行并贴结果）**：
  - `cd bedcode-desktop/src-tauri && cargo test` 全量（lib + 集成 target；用 `~/.cargo/bin/cargo`，勿把 toolchain bin 前置 PATH）
  - `cargo fmt` / `cargo clippy` 自查（非 CI 门禁）
  - 前端未改 → 不跑 vitest（写明跳过理由）
- **完成定义核对（spec §1.2 六条硬判据）**：
  - [ ] `rg "crate::wasm_core::manager" host_api/`（生产源码）零命中
  - [ ] `rg "crate::wasm_core::manager" security/`（生产源码）零命中
  - [ ] `monitor.rs` 不再内联引用 manager::task（经 MetricsSource 注入）
  - [ ] 无 WIT / ABI / 权限词汇 / wire 协议变更（git diff 不含 wit/、permission vocabulary）
- **文档记账**：
  - [ ] `bedcode-desktop/docs/code-map.md` 插件系统章节：模块结构按新依赖方向更新（context.rs / runtime_util / storage 归属、host_api 零 manager、api_bridge 归位 manager）
  - [ ] spec.md Status 更新为 done，各票 Status 更新为 closed/done
  - [ ] 根 `CHANGELOG.md` 按既有格式补一条（范围限桌面内重构，无跨端影响）
  - 若本次引入了新的架构决策（如「消费方定义接口 + 两阶段注入」上升为通用模式），在 `docs/adr/` 补充或修订既有 ADR；纯机械归属调整则写 code-map 即可
- **提交纪律（§11）**：按功能分 commit（如 01-03 中立化 / 04-05 上下文与接口隔离 / 06-07 归属与策略 / 08 文档），conventional commits 格式，禁止 Co-Authored-By

**验收：**

- [ ] `cargo test` 全量满绿（贴出通过项/失败项统计）
- [ ] 六条硬判据全部达成，rg 证据贴出
- [ ] code-map / CHANGELOG / spec 已更新；提交完成，工作区干净