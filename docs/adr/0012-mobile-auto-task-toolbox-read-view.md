# 移动端 auto-task 工具箱页 —— 桌面端数据只读视图，WS 推送驱动刷新

---
status: accepted
---

移动端 auto-task 插件此前只有终端工具栏按钮 + 悬浮队列面板（会话级）。为让手机能查看全局任务记录与定时自动任务，我们在移动端工具箱页新增 auto-task 入口（页签式二级页：**任务记录 + 定时自动任务**），数据全部来自桌面端，**移动端不落任何本地状态**——与 ADR 0008（预设任务执行状态本地化）形成对照：预设任务是手机本地轻量数据（卸载即清零），而任务记录/定时自动任务是桌面端 auto-task 插件权威管理的实体，手机只做视图。

## Considered Options

- **移动端本地镜像**（任务记录/定时任务在手机 SQLite 落副本）：离线可看，但引入跨端对账、schema 同步、生命周期一致性三大问题；任务记录本质是桌面端日志档案，无离线价值。否决。
- **HTTP 轮询刷新**（30s）：实现简单，但定时任务触发是分钟级低频事件，轮询空转浪费流量与桌面端查询；且推送链路（见下）已存在，零成本复用。否决。
- **WS 事件推送刷新**（选定）：桌面端 auto-task WASM 插件 → 宿主 sync_handler → WS SyncData 广播 → 移动端 Rust → `app.emit` 的推送链路**早已存在**（任务队列/定时/状态三类事件），移动端插件仅需 `context.events.on()` 订阅，宿主零改动。事件驱动刷新 + 下拉刷新兜底 + 重连重拉补偿断线期丢失的事件。

## Consequences

- 桌面端 auto-task 插件需在 `handle_http_endpoint` 补 3 个 HTTP 端点（移动端唯一数据通道）：
  - `GET task-history/list`（复用 `list_task_history`，支持 status 筛选 + limit/offset 分页）
  - `GET scheduled-jobs/list`（复用 `list_jobs`）
  - `POST scheduled-jobs/create`（复用 `create_job_with_broadcast`；移动端表单：名称 + 会话配置 + 本地时间→UTC 触发时刻 + prompts）
- 移动端工具箱页两个页签：任务记录（全局列表 + 状态筛选 chips + 加载更多分页 + 下拉刷新，无统计）、定时自动任务（只读展示 + 创建提交桌面端）；删除/重置/统计 v1 不做。
- 任务队列仍由终端悬浮面板管理（会话级、可操作），不进工具箱页，避免双入口双状态。
- 刷新订阅 3 个 Tauri 事件：`ws_sync_task_scheduled_changed`、`ws_sync_task_queue_changed`、`ws_sync_task_status_changed`，500ms 去抖合并突发；连接重连（`isConnected` 变 true）时重拉当前页签。
- 定时自动任务创建是唯一写操作，提交到桌面端由桌面端调度执行——"手机看，桌面管"的远程终端定位。
