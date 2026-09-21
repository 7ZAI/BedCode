# 06: 前端插件通道身份绑定（P0-5）

**What to build:** 前端插件代码无法再以他人身份调用宿主命令面——`plugin_*` Tauri 命令的调用方身份由宿主绑定，而非前端自报参数。修完后「Rust 端最终仲裁」对**前端通道**同样成立（目前只对 guest/WASM 通道成立）。

**Blocked by:** 无（但「是否实现 isolated sandbox」含产品裁决，见需裁决项）

**Status:** ready-for-agent

## 现状（已复核）

- `manager/api_bridge.rs:130-147`（`plugin_storage_get/set` 等）门禁 = `is_activated(参数里的 plugin_id)` + `permission.check(参数里的 plugin_id, ...)` → 查的是**被冒名者**的状态与权限；同 webview 内任一插件前端可直接 `invoke('plugin_storage_get', {pluginId:'受害者', key})`；
- `api_bridge.rs:254` 注释「前端无法伪造 plugin_id」不成立；
- `sandbox: 'isolated'` 只存在于类型定义（`src/plugin/types.ts:26`），`src/plugin/loader.ts:51` 把所有非 inline 直接跳过 → 该字段是虚假承诺，且**所有插件前端代码与宿主同权共生**（无 iframe/worker 隔离）。

## 需裁决项

1. 身份绑定方式：(A) 激活时宿主为每个插件前端签发一次性 channel token，`plugin_*` 命令按 token 反查 plugin_id（推荐，改动集中在 api_bridge + `PluginContext` 构造）；(B) 前端所有插件 invoke 经 `src/plugin/context.ts` 收口并由宿主校验「调用来源脚本 URL ∈ 该插件目录」（webview 内可被绕过，需先验证）；
2. `sandbox` 字段：真正实现 `isolated`（iframe + postMessage 桥，工作量另一票）还是**删除该字段**并明确「前端不做隔离，安全边界只在 Rust 端 + WASM 端」（诚实且省事，但要确认产品接受）；
3. 前端面是否需要与票 03 的审批联动（前端 API 面 = `contributes` + `PERMISSION_API_MAP`，高危 UI 位是否单独确认）。

## 验收

- [ ] 契约测试：以他人 plugin_id 调用 `plugin_storage_get` / `plugin_invoke` / 其余 `api_bridge` 命令 → 拒绝（先红后绿）
- [ ] `api_bridge.rs` 全部命令按裁决项 1 的机制改造，注释与实际一致（`api_bridge.rs:254` 的错误论断必须删除或改为真约束）
- [ ] `sandbox` 字段按裁决项 2 处置（实现或删）——禁止保留未实现的安全承诺字段
- [ ] 内置插件前端功能零回归（会话/终端/文件传输/AI 四个插件的 storage、invoke、事件订阅路径手测或测例覆盖）
- [ ] i18n：新增的用户可见错误文案同步 zh-CN 与 en
- [ ] 门禁：`pnpm run test:run`（`--pool=forks`）+ `pnpm exec eslint .` 0 error + `cargo test`（改了 Rust）

## Comments

- 2026-09-21 立项：来源 spec §4-P0-5 与 §2「前端命令面 plugin_id 自报」。
