# 01 — Android mDNS 多播锁重入消除（激活失败根因候选）

**What to build:** 消除 `mdns_browse` 内 `tauri::async_runtime::block_on(multicast_lock_acquire())` —— 该宿主函数违反项目自设约束「禁止在运行时内使用 block_on（会 panic）」（`manager.rs:152`），在 async command 路径（tokio worker 上调用 wasm activate）下存在 reentrancy panic 风险。改为 fire-and-forget 异步获取，使 file-transfer 激活不再因 mDNS 路径崩溃。

**准确度注记**：panic 是否每次必现取决于 tauri 全局 runtime 与 app 运行时的同一性等上下文（spec §1.2 准确度说明），静态分析无法 100% 定论；本票消除该反模式本身，并以「已进入 runtime 上下文」的用例实证（改动前可触发、改动后不触发），而非赌它不触发。

设计依据见同目录 `../spec.md` §1.2、§4 D1。

**Type:** task
**Status:** resolved
**Blocked by:** None — can start immediately.

- [x] `mdns_browse` 内不再同步 `block_on` 多播锁获取；改为 `tauri::async_runtime::spawn` fire-and-forget 调起（不依赖「是否处于 runtime」判断）
- [x] 多播锁获取保持 best-effort：失败只 warn 不阻断 browse（缺锁仅退化收包）
- [x] 同一条 browse 生命周期内锁获取至多一次（幂等守卫），避免重复唤醒
- [x] `mdns_browse` 签名与返回不变，`device_bridge::start_browse` 零改动
- [x] cargo test：在**已进入 tokio runtime 上下文**的用例中调用 `mdns_browse`，断言不 panic、不嵌套 block_on（改动前该场景可触发 panic）

## 验收
- Android 真机（或 adb 真机 / emulator）：file-transfer 启用不崩溃，激活走通。
- cargo test 全绿。

## Comments

## Comments

实现（2026-09-06）：
- `mdns_browse` 内 `tauri::async_runtime::block_on(multicast_lock_acquire())` 已移除，改为 `tauri::async_runtime::spawn` fire-and-forget（不阻塞当前线程、不嵌套 block_on、不依赖「当前是否已在 runtime」）；注释记录每次 browse 至多触发一次获取 + Kotlin 侧 acquire 幂等。
- 非 Android 平台不再 cfg 屏蔽：stub 立即返回 Ok(false)，同走 spawn 保持单一代码路径（可测）。
- 新增 `mdns.rs` 测试模块 3 例：runtime worker 内调用不 panic、纯 std 线程（runtime 外）调用不 panic、browse→stop_browse 3 轮循环 BROWSERS 表不增长且重复 stop 幂等 false。全部通过（src-tauri cargo test 328 全绿）。
- 待办：Android 真机 file-transfer 启用走通（本机无设备，留待人工验收）。
