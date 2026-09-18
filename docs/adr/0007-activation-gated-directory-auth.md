# 插件激活期集中目录授权（activation-gated directory auth）

**Status**: accepted

## Context

宿主 fs_auth 弹窗机制（路径白名单 → 插件白名单 → 用户弹窗授权，前缀制持久化）此前从未被任何插件实战验证过，移动端甚至没有授权弹窗组件。AI Chatbox 重构后对话日志落盘到数据目录，首次真正依赖 fs_auth 的目录写权限。

## Decision

AI Chatbox 插件（双端）在 `activate()` 时执行集中目录授权：

1. 计算数据目录（桌面 `{HomeDir}/.bedcode/ai-chatbox/`、移动 `{AppDownloadsDir}/ai-chatbox/`）
2. 调用宿主批量授权接口 `fs_request_auth([dataDir])`——一次弹窗、前缀制持久化，授权覆盖整个数据目录后续读写
3. 同意 → `store::init()` 建缺省文件 → 激活成功；拒绝或 30 秒超时 → 返回 Err，激活失败进入 Error 状态，错误信息含具体路径并提示"在插件设置中重新启用以再次授权"
4. 不把 `com.bedcode.ai-chatbox` 加入任何宿主白名单——保持走弹窗授权，作为 fs_auth 机制首次实战验证

## Considered Options

- **懒授权（首次写文件时逐个弹窗）**：每次写对话日志都可能弹窗打扰，且无法在激活时给出明确的数据落盘位置承诺。
- **白名单直通**：跳过弹窗，但违背"对话日志落盘需经用户同意"的诉求，fs_auth 机制继续无人验证。

## Consequences

- 拒绝授权 = 插件无法激活，重新启用可重试（宿主无"拒绝记忆"）。
- 未授权前每次启动都会弹窗（30 秒超时自动拒绝）——宿主既有行为，体验问题如突出则单独立项。
- 用户后续在设置中撤销授权 → 插件写文件失败，按错误关键词分类提示"目录授权已失效，请在设置中重新授权"。
- fs_auth 的批量路径弹窗（paths 数组）首次实战，桌面/移动端 FsAuthDialog 均支持多路径展示。
