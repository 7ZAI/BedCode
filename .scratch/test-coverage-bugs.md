# 测试覆盖新增过程中发现的既有代码 Bug

## [已修复] [移动+桌面] model/message.rs + server/ws/message.rs - from_ws_message(Close) 的 message 混入关闭码后缀
- 状态: **已修复**（两端同时修复，见 commit 未提交工作区）
- 文件: model/message.rs:845 / server/ws/message.rs:834（`reason.map(|r| r.to_string())`）
- 测试: test_from_ws_message_close_reason / from_ws_message_converts_close_to_error
- 现象: Close 帧携带 reason="going away"、code=Normal 时，生成的 Error.message 为 `"going away (1000)"`
- 期望: message 应为纯 reason `"going away"`
- 根因: tungstenite 0.24 的 `CloseFrame` Display 实现为 `"{reason} ({code})"`，用 `to_string()` 会附加关闭码；两端（桌面/移动）同源代码均受影响
- 修复: 改用 `r.reason.to_string()`（CloseFrame 有 pub reason 字段）；两端测试断言同步改回纯 reason，验证通过（桌面 313 ✓ / 移动 259 ✓）

## [已修复] [桌面插件] src/plugin/message_bus.rs - publish 在 dispatcher 未注入时提前返回，静态订阅者消息也被丢弃
- 状态: **已修复**（publish 改为按订阅者类型分流：dispatcher 仅 WASM 投递需要；静态订阅者走 Rust callback 不受影响）
- 文件: src/plugin/message_bus.rs:175（`let Some(dispatcher) = dispatcher else { ... return; }`，在读取订阅者之前）
- 测试: test_static_subscriber_receives_without_dispatcher（已移除 #[ignore] 并激活，另补充 WASM 订阅者被跳过的断言）
- 现象: 仅订阅了静态订阅者（BusSubscriber::Static，走 Rust callback，不依赖 WASM dispatcher）时，若 dispatcher 尚未注入，publish 直接丢弃消息，静态订阅者收不到
- 期望: 静态订阅者走 on_message callback，与 WASM dispatcher 无关；应仅在存在 Wasm 订阅者时才需要 dispatcher（或先按订阅者类型分流）
- 修复: dispatcher 改为 Option 惰性解包，仅在 BusSubscriber::Wasm 分支内使用；未注入时跳过 WASM 订阅者并警告，静态订阅者正常投递。验证：cargo test plugin::message_bus = 13 passed / 0 ignored

## [已解决] [桌面插件] src/plugin/wasm_runtime/host_impl/log.rs - 测试代码编译错误（CapturedEvent 缺 Clone），阻塞全量 cargo test
- 状态: **已解决（过时）**——该条目系并行编辑中间态，worker 完成 log.rs 测试后全量 cargo test 编译通过（451 passed / 1 ignored）
- 文件: src/plugin/wasm_runtime/host_impl/log.rs:184（`events.lock().unwrap().clone()`）
- 测试: capture() 辅助函数（mod tests 内，未提交工作区新增 151 行测试代码）
- 现象: `error[E0599]: the method clone exists for struct MutexGuard<'_, Vec<CapturedEvent>>, but its trait bounds were not satisfied` —— CapturedEvent 未派生 Clone，Vec<CapturedEvent>::clone() 无法编译；导致整个 lib test 二进制编译失败，所有模块测试（含 host.rs 新增测试）无法运行
- 期望: CapturedEvent 派生 Clone（或 capture() 改用其他取回方式）
- 说明: 工作区未提交改动引入（git status 显示 log.rs +151 行），与本次 host.rs / api_bridge.rs 测试新增无关；为验证 host.rs 测试，已临时 git stash 该文件（验证后已恢复原状），遗留给主 agent 处理
