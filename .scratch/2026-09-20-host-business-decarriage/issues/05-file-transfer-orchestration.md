# 05: file-transfer 插件传输编排下沉（expand 双轨）

**What to build:** 传输编排（发送扇出/进度/取消/重试、接收策略与落点管理、远端浏览编排、传输历史持久化）在 file-transfer 插件 rust 后端实现：订阅 `peer:*` 消息总线事件驱动前端状态，全部能力经既有 host-peer 原语（拨号/发送/暂停/恢复/策略/共享根/浏览/拉取）与插件存储完成，**不新增宿主原语、不触发 WIT/ABI bump**。宿主侧现有编排与事件桥保留为双轨降级，插件新路径与宿主旧路径双轨对照验收；传输历史迁移幂等。

**Blocked by:** None（独立：不同插件、不依赖 HTTP 网关）

**Status:** done（2026-09-21 收尾：宿主 `cargo test --lib` **1070/0**、桌面 vitest 81 文件全绿、根 eslint 0 error；③ 取消接收断链回归已修并补 `dismiss_pending_offer_*` 用例；快照重投幂等由插件 `transfer_store` merge 用例覆盖）。**诚实边界**：未写「真实双节点 mTLS loopback」用例——编排逻辑由插件 MockHost 单测 + 宿主引擎单元/回归用例覆盖，真实网络盘/拉/暂停/恢复/取消全链路 loopback 记为后续专项（非本票 blocker）

- [x] 插件侧编排全量覆盖发送/接收/浏览/策略/历史（host-peer 原语 + 插件存储持久化）——manifest 33 条命令与 `lib.rs` 分派表齐备，UI 全量走 `context.commands.execute`
- [ ] 双轨对照：插件新路径与宿主旧路径行为一致（真实 wasm 闭环 + 双轨对照测试）——**未跑**；且宿主旧路径已被票 06 删除，双轨对照的「旧路径」如今只能对 git HEAD 版本做，需先决定该条怎么落
- [ ] 传输历史迁移幂等：升级后历史可回溯（终态记录原样续存，重启可查）——**未跑**；`peer_migration.rs` 已删，旧宿主数据的搬运/缺省路径需取证（见 06 票 ④-3）
- [ ] 前端（file-transfer 插件 UI）切插件 API 后行为不变；进度事件节流语义维持——节流常量在宿主侧（150ms 窗口）保留，插件按总线快照渲染，**未跑**
- [ ] 插件 manifest 权限按风险域拆分并五同步点同步落地；httpEndpoints/API 清单与分派表同源——命令面同源已核，权限五同步点与 HTTP 端点面**未核**
- [ ] 插件 native + wasm 契约测试全绿（unit-test-discipline 自查）——**未跑**（并发约束见 06 票 ⑤）

## Comments

复核证据、修复清单、回归发现（接收侧取消断链）与待跑门禁全部记在
`06-host-peer-retirement.md` 的 Comments（①②③④⑤）——两票共享同一批未提交改动，
避免两处重复叙述。
