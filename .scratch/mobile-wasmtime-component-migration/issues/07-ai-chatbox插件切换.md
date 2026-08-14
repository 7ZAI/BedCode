# 07 — ai-chatbox 插件切换

**What to build:** 依赖面中等（Http/Config/Fs/Log）的内置插件切换到新 SDK：业务代码零改动，重新构建为组件产物，真机全流程回归（激活、命令、配置读取、文件读写路径）。

**Blocked by:** 03 — 宿主 host 接线与业务方法；05 — SDK 构建链

**Status:** done — 2025-08-14 真机回归通过（Pixel_8 模拟器，logcat + CDP 双证据链）

- [x] 产物为组件二进制（魔法字节 `0d 00 01 00`）且随构建链自动产出
- [x] 真机/模拟器：插件正常激活，对话命令、模型列表、配置读写可用
- [x] 迁移中暴露的任何残存旧 ABI 调用在编译期报错并已解决

---

## 交付物

| 项 | 说明 |
|----|------|
| `src-tauri/resources/plugins/mobile/com.bedcode.ai-chatbox/` | `bedcode-plugin build --resources-dir` 重出组件产物（512523 bytes，`00 61 73 6d 0d 00 01 00`）+ 前端 dist（index.js / wasm-DDgzZJey.js）一并重出 |
| 业务代码 | **零改动**（沿用 06 已实证的 wasm_entry 组件形态） |

## 验证记录

### 编译期（验收项 3）

- `touch src/lib.rs && cargo build --release --target wasm32-unknown-unknown` 强制重编：零警告零错误（当前 SDK 组件 bindgen 形态）
- 宿主侧 293 测试基线不受影响（本轮零 Rust 代码改动）

### 真机回归（Pixel_8 模拟器，`npm run tauri:android:dev:log`）

**logcat 证据链（进程 21345）：**

```
09:18:38 [PluginLoader] WASM plugin loaded: com.bedcode.ai-chatbox v1.0.0-beta   ← 组件产物生产路径实例化
09:24:29 fs_auth: user responded (batch) allowed=true                            ← 目录授权（Download/ai-chatbox）
09:24:29 [plugin:com.bedcode.ai-chatbox] Plugin activated (wasm, mobile)         ← 宏内 activate + HostLog 接线
09:24:29 WASM plugin activated plugin_id=com.bedcode.ai-chatbox                  ← manager 状态机
09:24:29 [PluginLoader] Plugin frontend loaded: com.bedcode.ai-chatbox
09:24:46 store: data dir initialized + host_config_get app.downloads_dir         ← 记住路径后免弹窗二次激活
09:27:46 reqwest::connect: starting new connection: https://api.openai.com/      ← chat 命令 → HostHttp 真实请求
09:28:08 Streaming HTTP request failed ...(chat/completions)                     ← 模拟器无外网，错误链路正常回传
```

**CDP 前端流程验证：**
- 插件管理页启用 AI Chatbox（Toggle）→ enabled 持久化 `enabled=true`
- 底部导航出现插件 navTab「AI」（贡献生效）
- AI 对话页渲染（ChatView）→ 配置模型 → 选 OpenAI 模板 → 表单保存
- 返回聊天页读回「OpenAI 测试 / gpt-4o-mini」（**配置读写全链路**：表单 → invoke → WASM 命令 → HostConfig/HostFs 落盘 providers.json → 读回显示）
- 发送消息 → 用户气泡 + `Streaming HTTP request failed` 错误气泡（**chat 命令链路**：invoke → WASM → HostHttp 请求发出 → 错误捕获回传 UI，无组件层异常）

**全程无 Error 上报**（除预期项：file-transfer 旧 core 产物降级 = 08 的范围；无外网导致的 HTTP 网络错误）。

## 过程发现与决策

1. **组件产物会被裸 cargo build 破坏（重要，08 必读）**：`bedcode-plugin build` 的 componentize 是**原地覆盖** `rust/target/.../<lib>.wasm`。之后若直接跑 `cargo build --release --target wasm32-unknown-unknown`（如为了验证编译），产物会**变回 core 模块**；而 `dev-run.js` 的 watch 模式每次启动都会把 `rust/target` 的 wasm 复制进 resources 目录 → 资源被旧 core 产物覆盖 → 真机加载报 `failed to parse WebAssembly module`。
   **规则：强制重编验证后必须重跑一次 `bedcode-plugin build`（componentize 幂等，~4s）再部署；任何裸 cargo build 后同理。**
2. **fs_auth 目录授权弹窗是 activate 前置**：ai-chatbox 激活时请求 `Download/ai-chatbox` 目录授权，30s 无响应即超时拒绝（`activate failed: 目录授权被拒绝`，前端回滚）。**弹窗出现后需立即点击「允许」**（本轮用 CDP 轮询弹窗出现 + 立即 adb tap 物理坐标解决）；勾选「记住此路径」后后续激活免弹窗。
3. **CDP 物理坐标换算**：Tauri Android WebView dpr=2.625（Pixel 8），DOM CSS 坐标 × dpr = 设备物理坐标（adb tap 用）；uiautomator 对 Tauri WebView 是 NAF 不可用。
4. **dev 进程被 timeout 杀死会导致懒加载路由失效**：主 bundle 已驻留内存但懒加载 chunk（如 PluginView.vue）fetch 不到 → 点击无响应。回归期间 dev 进程必须保持存活（后台 nohup 运行，结束再杀）。
5. **插件管理页全量状态同步**：`loadPlugins()` 会按后端 enabled 状态重放所有插件（触发一次全量 deactivate→activate 风暴），属正常行为；记住路径后第二次 activate 无弹窗。

## 下一步

ticket 08（file-transfer 切换，依赖面最大：Bus/FileService/Transfer/Http + 钩子）；注意先重出其 dist/ 前端产物（历史遗留 index.js import timeout 疑为此因），并按本 ticket 发现 #1 的规则确保 wasm 为组件产物。
