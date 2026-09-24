# wasm_core 解耦重构 — map

Notes / Decisions-so-far / Fog 跟踪文件（供 /wayfinder 与后续 agent 使用）。

## Decisions so far

### 2026-09-24 诊断与方案确认（本文档前身）

- 用户指令：检查 wasm_core 耦合度，用合适设计模式解耦；接口隔离「改动面大也要进行」。
- 诊断：manager ↔ host_api 双向环（C1-C7），详见 spec.md §1.1。
- 方案 8 票拆分获批（依赖顺序 01→08；02/03 与 01 可并行）。
- 设计模式：04 DIP、05 ISP、07 Strategy+Registry+DIP、02 Observer/Registry、01/03 职责重归属。
- 本批不做：C4 activation purge 观察者化、C5 api_bridge 窄接口深化（后续批次）。
