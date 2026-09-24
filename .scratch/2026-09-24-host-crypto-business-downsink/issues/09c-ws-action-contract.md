# 09c: WS 动作词表退役 · contract（删除宿主硬编码业务词表 switch）

**What to build:** 收尾 expand–contract：所有存量动作完成迁移后，删除宿主侧硬编码业务词表的 switch 分发，只保留通用声明式路由。宿主 WS 层不再内联任何业务动作名语义（对齐「宿主只提供最基础 POSIX 级 API、业务靠插件」）。落地后 grep 断言宿主无业务词表 switch；相关 `enums/` 硬编码词表类型随之后置/移除（线协议形状处置并入 08 口径）。

**Blocked by:** 09b

**Status: ✅ done（2026-09-24）**

- [x] `services/session_control.rs` 旧 `handle_control` 业务 switch 删除，重写为传输面转发层（声明闸门 + 转发 + 回包信封）；grep 断言宿主 WS 层无业务词表分发
- [x] 业务动作词表不再由宿主 switch 持有：词表解释唯一在插件 `ws_control`（manifest 声明端点驱动宿主可达性）
- [x] 全量门禁：宿主 lib 1062/0 + 集成 8 target 全绿（`pty_session_chain` 经转发层等值）+ 插件 303/0 + SDK 117/0
- [x] 传输面契约类型（`Message` / `SessionControlAction` / `SessionSummary`）保留宿主持有（H2），业务语义零残留
