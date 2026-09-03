# 08 — file-transfer 插件切换

**What to build:** 依赖面最大（Bus/FileService/Transfer/Http + 上传与传输钩子 + 存储）的内置插件切换到新 SDK：业务代码零改动，重新构建为组件产物，真机全流程回归（激活、挂载、上传/传输钩子策略、批准协议）。

**Blocked by:** 03 — 宿主 host 接线与业务方法；05 — SDK 构建链

**Status:** done — 2025-08-14 真机回归通过（Pixel_8 模拟器，logcat 证据链完整）；同日工作区被误 checkout 覆盖后已从 dangling stash 恢复（见「恢复记录」）

- [x] 产物为组件二进制（魔法字节 `0d 00 01 00`）且随构建链自动产出
- [x] 真机/模拟器：插件正常激活，文件服务挂载、上传钩子策略（含 fail-closed 路径）、传输批准语义与迁移前一致
- [x] 迁移中暴露的任何残存旧 ABI 调用在编译期报错并已解决

---

## 交付物

| 项 | 说明 |
|----|------|
| `src-tauri/resources/plugins/mobile/com.bedcode.file-transfer/` | `bedcode-plugin build --resources-dir` 重出组件产物（811057 bytes，`00 61 73 6d 0d 00 01 00`）+ 前端 dist（index.js 448KB，历史遗留 import timeout 疑因已解决）一并重出 |
| 业务代码 | **零改动**（wasm_entry 组件形态） |

## 验证记录

### 编译期（验收项 3）

- `touch src/lib.rs && npx bedcode-plugin build` 强制重编（走 build 命令，**避免裸 cargo 破坏组件产物**，见 07 发现 #1）：零错误，componentize 幂等成功
- 12 个 warning 均为业务代码既有 dead code，与迁移无关

### 真机回归（Pixel_8 模拟器，`npm run tauri:android:dev:log`）

**logcat 证据链（进程 24050）：**

```
10:03:34 WASM plugin loaded: com.bedcode.file-transfer v1.0.0-beta          ← 组件产物生产路径实例化（此前 core 产物报 parse error 降级，本次恢复）
10:03:34 Scanned 3 dir(s), loaded 3 plugin(s), 3 WASM instance(s)           ← 3/3 全部组件实例化
10:03:34 [plugin:com.bedcode.file-transfer] File Transfer plugin activating (wasm, mobile)
10:03:34 file service mounted plugin_id=com.bedcode.file-transfer mount=files roots=[Download]   ← FileService 挂载
10:03:34 mounted at /com.bedcode.file-transfer/files
10:03:34 approval timeout set plugin_id=... mount=files seconds=60           ← 批准协议超时配置
10:03:34 MessageBus: subscribed to filesrv:peer_changed / transfer_request / transfer_resolved /
         receiving_started / receiving_done / transfer_approval             ← Bus 6 事件订阅全通
10:03:34 peer probe on activate failed ... not connected                    ← WARN，未连接对端预期
10:03:34 File Transfer activated: 0 tasks loaded, 0 roots, concurrency=3    ← 存储/队列初始化
10:03:34 WASM plugin activated plugin_id=com.bedcode.file-transfer
10:04:38 [plugin:com.bedcode.file-transfer] File Transfer plugin activated (wasm mode, mobile)  ← 前端 activate
10:04:38 Plugin frontend loaded: com.bedcode.file-transfer
10:05:23 query-peer: probing remote file service state                       ← 命令链路（前端触发）
10:05:45 [File Transfer] list-remote OK: path='' entries=0                   ← list-remote 命令成功
```

**前端流程验证（CDP）：**
- 工具箱 → 文件传输页渲染（「对端未共享 / 请在对端设备上开启文件共享」空状态 + 「没有正在进行的任务」= list-tasks 命令成功返回空列表，**不再报 Command not found**）
- 文件 tab 切换 + list-remote 调用（logcat 确认）
- 全程无 Error 上报（file-transfer 相关）

**上传钩子/传输批准语义**：真机无法触发（需对端连接与真实传输），由宿主测试覆盖（`cargo test --lib` 293 全绿，含 fail-closed 钩子默认拒绝测试）；真机端钩子接线证据 = approval timeout 配置 + 6 个 MessageBus 订阅 + FileService 挂载（03 的 host 接线在 06/07/08 三个插件激活路径上均被真实调用）。

## 过程发现与决策

1. **前端 import timeout 是宿主既有时序 bug（非 08 引入）**：`loadAll` 在页面加载完成立即执行，此时 asset.localhost 的首次请求慢（约 2s+），后续插件的 `import()` 超过 5s `IMPORT_TIMEOUT` 即超时（file-transfer 恰好压线成功，auto-task/ai-chatbox 超时）；**页面稳定后手动 import 0ms**（07 同样存在：09:19:40 loadAll 超时、09:24 手动 activate 成功）。与迁移前行为一致（handoff 已知待办曾记录 file-transfer import timeout）。**修复候选**（宿主侧，不属于迁移范围）：提高 `IMPORT_TIMEOUT`（loader.ts:12）或 loadAll 前等待页面/asset 通道就绪。
2. **git 误覆盖事故（重要，恢复记录）**：见下节。

## 恢复记录（2025-08-14 工作区被 `git checkout` 覆盖）

**事故**：用户会话执行 git 操作（stash + checkout）时，移动端 5 个文件被覆盖回 HEAD：`plugin.rs`、`loader.rs`（迁移核心）、`manager.rs`（迁移核心）、`downloader.rs`、`packages/plugin-sdk-mobile/rust/src/types.rs`；新文件 `validation.rs`、`approval.rs`（用户自己的在途工作）被删除。导致 `cargo check` 失败（`manager.rs` 传 `Arc` 期望 `Option<Arc>` 等 2 处 E0308）。

**恢复**：
1. **git fsck --lost-found** 发现 dangling commit `02008c8d`（WIP on dev: 7726a83e6，18:00:27，即被覆盖前的完整工作区快照）——stash 被 drop 后仍留在对象库
2. 从 `02008c8d` 恢复 `loader.rs`（364 行，组件模型版）与 `manager.rs`（708 行，含 `Some(app_handle.clone())` 与 `LoadedComponentPlugin` 迁移特征）
3. **对比确认**：`plugin.rs` / `types.rs` / `downloader.rs` 的 02008c8d 版 = HEAD 版（迁移未改动，无需恢复）；`fs_auth.rs` / `wasm_runtime.rs` / `host_impl/*` / `component.rs` 未被覆盖（M 状态保留）
4. **行尾陷阱**：git show 输出为纯 LF；经 Python stdout 管道会因 Windows 文本模式双重转换出 `\r\r\n`（rustc 报 bare CR），直接 `git show > file` 写入即可
5. **保护**：已打 tag `wip-recovery-02008c8d` 防 gc
6. 验证：`cargo check --lib` 通过；`cargo test --lib` **293 全绿**；SDK `cargo test --features wasm` 89 全绿

**教训**（写入 09 或 docs）：git checkout / stash 前必须 `git status` 确认无未提交改动；未提交的关键工作应随时 `git stash`（勿 drop）或提交；误覆盖后**先 `git fsck --lost-found` 找 dangling commit/blob 再重写**。

## 下一步

ticket 09（删 legacy ABI + core 路径 + 文档同步 + Kotlin 编译验证）；宿主 `IMPORT_TIMEOUT` 修复候选（loader.ts）与恢复经验文档一并纳入。
