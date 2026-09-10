# 插件互调机制 —— manifest 声明 + 宿主注册表门禁 + JSON-RPC 2.0 + SDK 宏

---
status: accepted
---

BedCode 插件此前只能通过宿主消息总线广播事件（自由 topic + 自由 JSON，单向），插件之间"互相调用"没有门禁：任何插件可往任意 topic 发布任意载荷，未声明的接口也能被调用（或者更糟——没有"接口"概念，全靠口头约定）。计划任务插件（com.bedcode.scheduler）需要被其他插件以"调用方法并取回结果"的方式使用，这逼出了插件互调机制。我们决定：**插件对外可调用的 api 必须在 manifest 声明（宿主加载时登记），互调消息（`bedcode.api.` 前缀 topic）经宿主校验目标 api 已声明否则拒绝；消息形状采用 JSON-RPC 2.0（id/method/params/result/error）；SDK 提供 `#[plugin_api]` 宏，由 trait 生成实现方分派与调用方 client（相当于 IDL 生成），构建期比对 manifest 与实现防漂移。**

## Considered Options

- **纯 SDK trait 约定（宿主零改动）**：SDK 定义 trait 作为互调 API 的形状，消息仍走现有自由总线。问题：trait 是编译期/进程内概念，跨 WASM 组件边界不成立，只能约束消息形状；宿主不校验时任何插件仍可手写任意 topic 的 JSON，"限制胡乱调用"落空——只能防君子。否决。
- **组件模型静态链接（WASI import/export 链接）**：类型在编译/链接期锁定，最严；但要求构建时知道插件对，无法支撑"插件运行时装上就能被调"的运行时发现。否决。
- **宿主注册表 + 命名寻址（选定）**：Electron `ipcMain.handle`、VS Code `contributes.commands`、D-Bus introspection 的主流做法——调用入口必须在宿主注册过，未注册调用直接失败。"注册即声明，未声明不可调"由宿主强制。门禁取**层 1**：只校验目标 api 已声明，不校验调用方身份（本机插件可信，出站声明徒增负担）、不做版本化（YAGNI，出现破坏性变更再引入）。
- **消息形状**：比较自造 correlation id 约定与 JSON-RPC 2.0（LSP/D-Bus 生态标准），选后者——标准、可复用、错误语义完备。

## Consequences

- 宿主改动：manifest schema 增加 `api` 字段 + 插件激活时登记 api 清单 + `bus_publish` 对 `bedcode.api.` 前缀做目标校验（普通广播 topic 不校验，向后兼容）。
- SDK 改动：`#[plugin_api]` 宏（实现方 JSON-RPC 分派 + 调用方 client + 响应 topic 配对 + 调用超时）+ 构建期 manifest 比对。
- 计划任务插件声明 `schedule.add/remove/list/show/run/logs`，作为第一个实现者与验证载体。
- 未声明互调 API 的插件不受影响；现有事件广播（`filesrv:peer_changed` 等）保持原样。
- 互调调用为异步（总线单向 + 响应 topic 配对），SDK 隐藏异步细节，插件代码表现为 await 调用。
