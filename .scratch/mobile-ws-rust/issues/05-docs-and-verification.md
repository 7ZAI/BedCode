# 05: 文档同步 + 全量验证收尾

Type: task
Status: resolved（2026-09-12 晚；提交由桌面 agent 统一执行）
Blocked by: 04

## 范围
- `docs/knowledge/` 协议描述（TB v3 / 双速 / HTTP 历史）同步；移动端 code-map 相关条目更新
- `.scratch/mobile-ws-rust/spec.md` 核对落地（偏离处记录）
- 全量验证：两端 cargo test / vitest / eslint；`lens_diagnostics mode=all` 无 blocker
- 变更记录：根 CHANGELOG.md 记一次条目（协议 v3 + 移动端 WS 迁入）

## 验收
- 验证命令全绿；无 blocker
## 落地情况（2026-09-12）
- docs/knowledge/pty-output-pipeline.md 重写为 TB v3 全链路（桌面字节块队列/双速/HTTP 历史/移动端 Rust 链路/前端语义/协议表/测试/相关文档）✓
- 移动端 code-map：新增 Core Module「终端链路（TB v3 字节连续）」条目 + 前端终端链路改写 + Quick Navigation 两行 ✓
- spec.md 新增 §6 落地核对（6 条偏差 D-A1~A6：history-ready→命令返回值、历史主路径=WS 重播入缓存、ack 水位=max、invoke 返回值 camelCase 键、链路加密降级、store 增补）✓
- 全量验证：移动端 cargo test 282 / vitest 372 / vue-tsc 0 / eslint 0 error ✓；桌面端验证由在途桌面 agent 收尾（其 vitest 于 05:25 已跑）
- 根 CHANGELOG.md：**用户裁决不改**——2.1.0 为已发布日志，本次未发布变更不新增条目（先前误加 [Unreleased] 已回滚，git diff 与 HEAD 一致）✓
