# CLI（bedtask）：Rust bin + 插件生命周期安装/卸载 + 命令集

Type: task
Status: ready-for-agent

> 规格依据：`.scratch/task-scheduler-plugin/spec.md` §8。依赖 02 号 issue（HTTP 端点）。

## 任务

1. **Rust bin crate**（插件目录内，如 `plugins/task-scheduler/cli/` 独立 crate，产物 `bedtask(.exe)`）：
   - 薄客户端：localhost HTTP → 桌面端网关 `/api/plugin/com.bedcode.scheduler/...`
   - 端口：读环境变量/默认 8765（与宿主 `config_get(NetworkPort)` 对齐——桌面端未运行时 CLI 报 `desktop not running`）
   - 命令集（与 02 号 issue 的端点一一对应）：
     ```
     add --cron <6段> (--script <path> | --exec <cmd>) [--name] [--cwd] [--env K=V,...] [--timeout] [--once]
     list / show <id> / remove <id>
     edit <id> [--cron] [--exec] [--name] [--timeout] [--once/--no-once]
     enable <id> / disable <id> / run <id> / logs <id> [--limit N]
     ```
   - 输出：人类可读默认 + `--json`（agent 友好）
2. **插件包结构**：`plugin.json` + wasm 组件 + `cli/bedtask(.exe)`；构建发布流程（plugins/*/scripts/build.js 模式）把 CLI 产物纳入插件包
3. **安装/卸载生命周期**（插件 on_activate / on_deactivate）：
   - 安装：复制 CLI 到用户 bin 目录（Windows `%LOCALAPPDATA%\com.bedcode.app\bin\`；macOS/Linux `~/.bedcode/bin/`）+ 注册 PATH
     - Windows：用户级注册表 `HKCU\Environment\Path`（免管理员）+ 广播 `WM_SETTINGCHANGE`
     - macOS/Linux：`~/.local/bin/` symlink
   - 幂等：重复激活不产生重复条目；升级覆盖
   - 卸载：删文件 + 移除仅本插件添加的 PATH 条目（保留用户原有项）
   - PATH 操作为宿主侧实现还是插件内实现需评估：WASM 插件无注册表/文件系统写权限的直接通道（host-fs 有 write 但注册表无对应能力）——**宿主新增能力或宿主侧 helper**，实现者与 01 号 issue 协调（可并入 host-process 或独立 host-app 接口）

## 验收标准

- [ ] 全命令集对 02 号端点冒烟通过（含 `--json` 输出）
- [ ] Windows 安装：HKCU Path 出现 bedtask 目录且不重复；新开终端 `bedtask` 可用；停用插件后条目移除、原 PATH 其他项保留
- [ ] macOS/Linux：symlink 安装/移除正确
- [ ] 桌面端未运行：CLI 报错退出码非 0
- [ ] 插件包构建脚本产出含 `cli/bedtask`

## 相关代码

- 参照：`bedcode-desktop/plugins/auto-task/rust/src/hooks.rs`（插件对项目目录的安装/清理模式）、`plugins/*/scripts/build.js`
- 本插件新建：`plugins/task-scheduler/cli/`（bin crate）
