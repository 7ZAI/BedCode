# 02: trap 统一宿主日志入口

**What to build:** 插件调用发生 WASM trap 时，宿主侧始终有一条 error 级日志（含 plugin_id 与 trap 详情，含调用栈）——即使调用方静默忽略返回错误，崩溃证据也已落盘。AI agent 排障时只要 grep error 日志就能确认"哪个插件在哪个导出上崩了"。

**Blocked by:** 01（验收断言日志内容包含 01 提供的调用栈）

**Status:** ready-for-agent

- [ ] WASM 导出调用的 trap 分支（双层 Result 的 Err）统一补宿主侧 error 日志，携带 plugin_id 与 trap 详情（含调用栈）
- [ ] 双层 Result 语义不变：guest 自报失败（Ok(Err(msg)) 路径）仍只按既有级别记录，不升级为宿主 error
- [ ] 错误仍按既有控制流返回（日志为证据、返回为控制流），调用方行为零变化
- [ ] 测试：trap 场景断言宿主侧产生含 plugin_id 的 error 级记录；guest 自报失败场景断言不产生宿主 error
