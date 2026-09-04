# 08 — Linux 平台适配：发行形态 + 缓存清理目标

**What to build:** 在现有 v1（Windows 专用）的基础上扩展 Linux 桌面端发行形态 + 适配 Linux 用户域文件系统。具体三块：

1. **Linux 桌面端 bundle**：扩展桌面端 Tauri 2 build 配置，新增 Linux `deb` bundle 目标（系统依赖 webkit2gtk-4.1 跟 Tauri 2 默认走，不做 AppImage）。原本桌面端只 build Windows，新增后双平台。
2. **磁盘清理插件 Linux 规则子集**：新增 spec §3-Linux 子表，覆盖 XDG Base Dir 用户层缓存 + Chromium / Firefox 浏览器缓存。**不做** `/var/tmp`、**不做** Snap/Flatpak、**不做** systemd/journald。
3. **平台分发约束**：manifest 的 `wasiPreopenDirs` 通过 `cfg!(target_os)` 在 wasm host 加载插件时切换——Windows: `%USERPROFILE%/AppData/Local`；Linux: `$XDG_CACHE_HOME`（默认 `~/.cache`，尊重 `XDG_CACHE_HOME` 环境变量）。隔离区路径跨平台统一为 `${home}/.bedcode/cleaner-quarantine/`（Linux 即 `~/.bedcode/cleaner-quarantine/`）。

**Blocked by:** 04（隔离区与批次基础设施）/ 05（还原）/ 06（设计审查）/ 07（i18n 验收）

**依据决策（2026-08-26 用户裁定）：**
- 发行形态：Debian/Ubuntu **.deb**（不做 AppImage）
- WebKitGTK 版本：跟 Tauri 2 默认（不显式指定，目前 Tauri 2 Linux 默认 webkit2gtk-4.1）
- `wasiPreopenDirs`：仅 `$XDG_CACHE_HOME`（**不**含 `~/.local/share`）
- 规则子集：XDG Base Dir 用户层缓存 + Chromium / Firefox 浏览器缓存
- 隔离区路径：`~/.bedcode/cleaner-quarantine/`（与 Windows 同布局）

**Status:** ready-for-agent

## 工作分解

### A. 桌面端 Linux bundle 接线

- [ ] `bedcode-desktop/src-tauri/tauri.conf.json` 加 Linux bundle 配置（`bundle.targets: ["deb"]`，`bundle.linux.deb.depends` 列 webkit2gtk-4.1 / libgtk-3 / libayatana-appindicator3 等 Tauri 2 Linux 标准系统依赖）
- [ ] `bedcode-desktop/scripts/tauri-build.js` 加 Linux 分支（Windows-only 的 updater 签名解析逻辑不适用于 Linux；Linux 不需要签名密钥，跳过）
- [ ] CI / 本地构建文档（`AGENTS.md` Build & Run 节）补充 `pnpm run tauri:build -- --target ...` 在 Linux 上的命令字眼（如适用）
- [ ] Linux 端 `Cargo.lock` 与 webkit2gtk 系统依赖兼容性验证（Linux 上首次 cargo build 会拉 webkit2gtk-sys，需系统装 `libwebkit2gtk-4.1-dev` 等）

### B. 磁盘清理插件 Linux 适配

- [ ] manifest 模板接入 `cfg!(target_os)` 平台分发：Windows 走 `%USERPROFILE%/AppData/Local`；Linux 走 `$XDG_CACHE_HOME`（默认 `~/.cache`，尊重环境变量）
- [ ] 新增 Linux 规则子集（详见 spec §3-Linux）：
  - XDG Base Dir 用户层缓存：`$XDG_CACHE_HOME` 内可识别的应用子目录（按 spec 子表逐项）
  - Chromium 缓存：`$XDG_CACHE_HOME/chromium/Default/Cache/**`、`.../Code Cache/**`、`.../GPUCache/**`（多 Profile 通配 `*/Cache/**`）
  - Firefox 缓存：`$XDG_CACHE_HOME/mozilla/firefox/*/cache2/**`（小写目录名，Linux 上 Firefox 实际路径）
- [ ] 硬黑名单 Linux 扩展：与 Windows 同步覆盖——`**/Cookies`、`**/Cookies-journal`、`**/Logins*`、`**/Web Data*`、`**/Bookmarks*`、`**/History*`、`**/Favicons*`、各应用顶层配置 / 本地数据库（如 `chromium/Default/Preferences`、`chromium/Local State`、`mozilla/firefox/profiles.ini`）
- [ ] 隔离区路径跨平台：Windows 与 Linux 均落到 `${home}/.bedcode/cleaner-quarantine/`，跨平台路径由 wasm host 通过 `${home}` 变量解析时插值
- [ ] 平台检测原语：manifest 暴露 `host.os`（已有），插件代码据此决定加载哪一份规则子集；不允许一个插件里硬编码两条平台的 glob 模式混在一起

### C. 验证

- [ ] Linux 真实环境端到端：Debian/Ubuntu 22.04+ 装机，`pnpm run tauri:build` 出 `.deb`，安装后激活磁盘清理插件扫一次 `~/.cache`，勾选若干项真实清理演示通过（move 入 `~/.bedcode/cleaner-quarantine/`）
- [ ] 黑名单单测（Linux 路径）：构造同时命中规则模式与黑名单的路径（如 `chromium/Default/Cookies`），断言强制跳过
- [ ] 平台分发单测：mock `target_os = "linux"` 时 manifest 解析出 `$XDG_CACHE_HOME`，mock `target_os = "windows"` 时解析出 `%USERPROFILE%/AppData/Local`
- [ ] `XDG_CACHE_HOME` 环境变量覆盖：测试设置 `XDG_CACHE_HOME=/tmp/custom-cache` 时插件识别到的根路径同步改变
- [ ] 隔离区惰性过期（Linux 路径同 Windows 行为）
- [ ] i18n key 同步 zh-CN 与 en
- [ ] 插件 `cargo test` + 宿主 `cargo test --lib` + 桌面端 `pnpm run test:run` 全绿
- [ ] §6.5 截图审查闭环：Linux 桌面端扫描结果页 / 目标勾选列表 / 隔离区页 / 置灰项展示，vision 评审无阻断问题，截图与结论已留档 `.scratch/disk-cleaner-plugin/reviews/`

### D. 文档与契约

- [ ] spec.md 同步修订：§2 加平台分发约束、§3 后追加 §3-Linux 子表、§5 隔离区路径跨平台统一为 `${home}/.bedcode/cleaner-quarantine/`、§7 验收清单补充 Linux 条目
- [ ] **ADR 入库策略**（**可选，不阻塞验收**）：与 v1 保持一致——`.scratch/disk-cleaner-plugin/spec.md` 是契约真源；ADR0023 v1 历史上**未入库**（仅 doc-tracking 保护路径下的工作副本），本次 Linux 增量若需独立 ADR，可按既有模式 `git add -f docs/adr/00XX-disk-cleaner-linux-platform.md` 入库（参考已跟踪的 8 个 ADR：0002/0008/0009/0010/0011/0014/0020/0021）；如不开新 ADR，spec §2/§3-Linux/§5/§7 即承担全部契约描述。**本 issue 不强制 ADR 入库**
- [ ] `docs/agents/issue-tracker.md` 如有元数据要求按需同步（无需新增字段）

## 验收标准（issue 完结条件）

- [ ] 上述 A/B/C/D 全部勾选
- [ ] 桌面端 `pnpm run tauri:build` 在 Linux 上能出 `.deb`
- [ ] 安装并真实清理过一次用户级缓存（move 入隔离区 + 还原链路验证）
- [ ] 黑名单 + 平台分发单测覆盖
- [ ] i18n 完整
- [ ] spec §6.5 截图审查闭环