# 票 06 — 文档：ADR 0054 + ADR 0042 增补 + 注释同步复核

Status: ready-for-agent
Type: task
Blocked by: 05

## 内容

- **新开** `docs/adr/0054-mdns-single-browse-per-type-and-fanout-hub.md`（跨 crate 机制不变量，spec §6 D6）：
  - 背景：根因（mdns-sd 每服务类型单 listener、后到覆盖；重传链保活旧 listener）；ADR 0042 只做了守护级共享，listener 级缺位
  - 决策：全进程每类型唯一 `daemon.browse()` 只由类型中枢代持；订阅者进程内 fan-out；requery 收敛中枢；锁白名单（`type_hub.rs` 唯一 browse / `engine.rs` 唯一创建守护）
  - 后果与不做项（spec §8）；与 ADR 0042 / 0022 的关系（零 WIT / ABI 变更，不触发 bump）
- `docs/adr/0042-*.md`「验证」或「与既有 ADR 的关系」节**增补一行执行记录**指向 0054
- **注释同步复核**（spec §8；票 02/03/04 已改，此处逐条复查）：peer-net `discovery.rs` 模块头、engine.rs `shared_daemon` 注释、移动 `plugin/host_domains/mdns.rs:16` 红线、ADR 0042 关联段落
- CHANGELOG **不写**（AGENTS §2：发布前一次性汇总）

## 门禁

- 文档与最终代码逐条核对；引用的路径 / 符号 / 行号真实可对照（AGENTS §0「文档字面 ≠ 事实」）
- 两端 code-map 若票 05 已登记，此处仅核对
