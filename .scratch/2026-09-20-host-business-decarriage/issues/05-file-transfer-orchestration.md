# 05: file-transfer 插件传输编排下沉（expand 双轨）

**What to build:** 传输编排（发送扇出/进度/取消/重试、接收策略与落点管理、远端浏览编排、传输历史持久化）在 file-transfer 插件 rust 后端实现：订阅 `peer:*` 消息总线事件驱动前端状态，全部能力经既有 host-peer 原语（拨号/发送/暂停/恢复/策略/共享根/浏览/拉取）与插件存储完成，**不新增宿主原语、不触发 WIT/ABI bump**。宿主侧现有编排与事件桥保留为双轨降级，插件新路径与宿主旧路径双轨对照验收；传输历史迁移幂等。

**Blocked by:** None（独立：不同插件、不依赖 HTTP 网关）

**Status:** ready-for-agent

- [ ] 插件侧编排全量覆盖发送/接收/浏览/策略/历史（host-peer 原语 + 插件存储持久化）
- [ ] 双轨对照：插件新路径与宿主旧路径行为一致（真实 wasm 闭环 + 双轨对照测试）
- [ ] 传输历史迁移幂等：升级后历史可回溯（终态记录原样续存，重启可查）
- [ ] 前端（file-transfer 插件 UI）切插件 API 后行为不变；进度事件节流语义维持
- [ ] 插件 manifest 权限按风险域拆分并五同步点同步落地；httpEndpoints/API 清单与分派表同源
- [ ] 插件 native + wasm 契约测试全绿（unit-test-discipline 自查）