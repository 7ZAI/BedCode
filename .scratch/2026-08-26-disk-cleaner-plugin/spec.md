# 磁盘清理插件（桌面端 WASI）实施规格

Status: ready-for-agent
Date: 2026-08-26
决策来源: grilling 会话（同日），安全模型另见 `docs/adr/0023-disk-cleaner-least-privilege-quarantine.md`

## 1. 定位

桌面端内置插件（WASI 组件）：面向 C 盘个人缓存数据与用户域系统残留的磁盘清理工具，对标 CCleaner 类工具的缓存清理能力。**注册表清理不在本期范围**，二期独立立项且强制带「备份→清理→一键还原」闭环。

## 2. 架构约束（硬性）

- 纯 WASI 插件：预打开目录内用原生文件 API（`fd_readdir` / read / remove）完成枚举与删除
- **零 WIT 改动、零 ABI bump**：不新增任何宿主接口函数；目录枚举靠 wasiPreopenDirs 解决
- manifest 声明 `"wasiPreopenDirs": ["${cacheRoot}"]`——`cacheRoot` 由 wasm host 在加载插件时按平台解析（详见下方「平台分发约束」）
- 预打开挂载发生在 activate；首次授权走既有 fs_auth 弹窗流程（`request-auth` + 授权持久化）
- 删除实现为「move 到隔离区 → 过期后删除」，插件代码中**不存在绕过隔离区的直接 remove 路径**

#### 平台分发约束（v1.1 起生效）

`wasiPreopenDirs` 解析在 wasm host（Rust 侧）按平台切换，插件代码只看到一个固定挂载点 `${cacheRoot}`，不感知平台差异：

| 平台 | `${cacheRoot}` 解析结果 | 依据 |
|------|-----------------------|------|
| Windows | `%USERPROFILE%/AppData/Local` | Windows 习惯，AppData/Local 是用户级缓存标准位置 |
| Linux（Debian/Ubuntu .deb） | `$XDG_CACHE_HOME`（默认 `~/.cache`，尊重环境变量） | XDG Base Dir 规范；Tauri 2 Linux 默认 webkit2gtk-4.1 |

平台分发的硬约束：

1. **禁止**插件代码内硬编码两条平台的 glob 模式混编（违反最小权限原则——无法保证模式只命中当前平台根）
2. **禁止**做 `/var/tmp`、`/tmp`、Snap/Flatpak 应用数据目录等「超出 XDG_CACHE_HOME」的目标（出圈，提权风险）
3. **禁止**给 wasiPreopenDirs 加 Linux 下的 `~/.local/share`（v1 决策明确只授权缓存，不授权用户数据）
4. **禁止**做 AppImage bundle（发行形态只承诺 .deb，AppImage 与 wasiPreopenDirs 行为差异需独立评审）
5. 平台检测走 `host.os` 原语（已有），插件据此加载对应规则子集；不允许运行时探测文件系统推断平台

## 3. 范围

### v1 范围内（可勾选目标）

| # | 清理目标 | 匹配模式（`{home}` = `%USERPROFILE%`） | 说明 |
|---|---------|----------------------------------------|------|
| 1 | 用户临时文件 | `{home}/AppData/Local/Temp/*` | 主收益项 |
| 2 | Chrome 缓存 | `{home}/AppData/Local/Google/Chrome/User Data/*/Cache/**`、`.../*/Code Cache/**`、`.../*/GPUCache/**` | 各 Profile 通配 |
| 3 | Edge 缓存 | `{home}/AppData/Local/Microsoft/Edge/User Data/*/Cache/**`、`.../*/Code Cache/**` | 同上 |
| 4 | Firefox 缓存 | `{home}/AppData/Local/Mozilla/Firefox/Profiles/*/cache2/**` | |
| 5 | IE/系统 INetCache | `{home}/AppData/Local/Microsoft/Windows/INetCache/*` | 低优先级 |
| 6 | 缩略图缓存 | `{home}/AppData/Local/Microsoft/Windows/Explorer/thumbcache_*.db` | Explorer 常占用 → 走跳过逻辑 |
| 7 | DirectX 着色器缓存 | `{home}/AppData/Local/D3DSCache/**`、`{home}/AppData/Local/NVIDIA/DXCache/**`、`{home}/AppData/Local/AMD/DxCache/**`（存在哪个算哪个） | 会导致游戏首次加载重编译，文案注明 |
| 8 | 每用户崩溃转储 | `{home}/AppData/Local/CrashDumps/*.dmp` | |
| 9 | Windows 错误报告队列 | `{home}/AppData/Local/Microsoft/Windows/WER/**` | |

### 置灰展示（不可勾选，标注「需要系统权限（暂不支持）」）

- `C:\Windows\Temp`
- `C:\Windows\SoftwareDistribution\Download`（Windows Update 缓存）
- Delivery Optimization 缓存

### §3-Linux 规则子集（v1.1 扩展）

仅 Linux 平台生效（`host.os == "linux"` 时加载）。**`{cacheRoot}` = `$XDG_CACHE_HOME`（默认 `~/.cache`，尊重环境变量）**。

| # | 清理目标 | 匹配模式 | 说明 |
|---|---------|----------|------|
| L1 | XDG 标准用户层缓存（应用自管理） | `{cacheRoot}/**` 内命中下列应用子目录的可识别缓存文件 | 主收益项 |
| L1.1 | Chromium 缓存 | `{cacheRoot}/chromium/*/Cache/**`、`.../*/Code Cache/**`、`.../*/GPUCache/**` | 多 Profile 通配（`*` 匹配 Default / Profile 1 / Profile N） |
| L1.2 | Firefox 缓存 | `{cacheRoot}/mozilla/firefox/*/cache2/**` | Linux 上 Firefox 实际用小写目录 |
| L1.3 | Google Chrome 缓存 | `{cacheRoot}/google-chrome/*/Cache/**`、`.../*/Code Cache/**`、`.../*/GPUCache/**` | Debian 包名 `google-chrome-stable` 的默认缓存路径 |
| L1.4 | Microsoft Edge 缓存（Linux 版） | `{cacheRoot}/microsoft-edge/*/Cache/**`、`.../*/Code Cache/**` | Edge for Linux 自 2023 起在 Debian 仓库可用 |
| L1.5 | Brave 浏览器缓存 | `{cacheRoot}/BraveSoftware/*/Cache/**`、`.../*/Code Cache/**` | Brave 用户群较 Chromium 主线更高 |
| L2 | 缩略图缓存 | `{cacheRoot}/thumbnails/**` | GNOME / KDE / XFCE 通用；常见占用大户 |
| L3 | 各应用一次性临时目录 | `{cacheRoot}/<app>/tmp/**`、`.../temp/**` | 按应用枚举命中（具体名单随浏览器升级微调） |
| L4 | 每用户崩溃转储 | `{cacheRoot}/**/*.crash`、`{cacheRoot}/**/*.dmp` | Electron 应用多 |

#### Linux 置灰展示（不可勾选，标注「需要系统权限（暂不支持）」）

- `/var/tmp` 中可识别项（出圈 + 需 root，不在 `$XDG_CACHE_HOME` 内）
- `/var/cache` 系统级包缓存（`apt/archives`、`PackageKit` 等，需 root）
- systemd journald 旧日志（需 root + journalctl 协作）
- Snap / Flatpak 应用缓存（`~/snap/**`、`~/.var/app/**`）——超出 XDG_CACHE_HOME 授权范围，且 Snap/Flatpak 自带清理入口

#### Linux 出圈（明确不做）

- `/var/cache` 系统包缓存清理
- systemd journald 旧条目
- Snap / Flatpak 应用缓存清理
- `/var/tmp` 中任何项
- 容器 / 镜像 / build cache 清理（Docker / podman / buildah 等）

### 出圈（明确不做）

- 注册表清理（二期独立立项）
- 启动项 / 开机自启管理
- 大文件分析 / 磁盘空间可视化
- 回收站清空（盘根 `$Recycle.Bin` 不在授权范围；Windows 有原生入口）
- Linux 上的 `/var/cache` 系统包缓存、systemd journald 旧条目、Snap/Flatpak 应用缓存、`/var/tmp`（出圈，需 root 或超出 XDG_CACHE_HOME）

## 4. 安全模型（四层防御）

1. **白名单**：预打开仅 `{home}/AppData/Local`，能力上限
2. **规则清单**：上表即清单，UI 逐项展示 + 勾选；实际删除的唯一真源
3. **硬黑名单**（优先级高于模式命中，命中即跳过）：
   - 浏览器：`**/Cookies`、`**/Cookies-journal`、`**/Login Data*`、`**/Web Data*`、`**/Bookmarks*`、`**/History*`、`**/Favicons*`
   - 通用：各应用顶层配置文件 / 本地数据库（如 `User Data/Local State`）；规则模式不得匹配 `{home}/AppData/Local` 直接子级的非 Temp 条目
4. **隔离区**：见 §5

## 5. 隔离区与批次

- 位置：**`{home}/.bedcode/cleaner-quarantine/<batchId>/…`**（batchId 为时间戳派生 ID）——跨平台统一路径
  - Windows：`%USERPROFILE%/.bedcode/cleaner-quarantine/`
  - Linux：`~/.bedcode/cleaner-quarantine/`（**不**在 `$XDG_CACHE_HOME` 内——隔离区是插件自管理数据，按 XDG 规范应放在 `$XDG_DATA_HOME/bedcode/`，v1 阶段为最小迁移成本复用 `${home}`，二期再考虑按 XDG_DATA_HOME 重定位）
- 批次元数据：每批一份 meta（JSON：类别、逐文件原始路径、大小、时间），还原按原始路径 move 回去；原路径已被占用则该文件留在隔离区并在结果中提示
- 保留期固定 **7 天**，不做成设置项
- 过期清理惰性执行：插件 activate 时 + 每次扫描开始时顺手清除已过期批次
- UI：隔离区页按批次列表（类别 / 文件数 / 占用空间 / 时间），支持整批一键还原；不做单文件挑选

## 6. 触发与交互流程

仅手动，四步流：**扫描 → 展示 → 勾选 → 清理**

- 扫描：遍历规则清单模式，按清理目标分组汇总（文件数 + 可释放空间）；需系统权限项固定置灰展示
- 结果页：默认全选可清理项；显示黑名单跳过计数与占用跳过计数（「N 个文件被占用已跳过」）
- 清理：先全部移入隔离区（当前批次），完成后展示释放空间摘要
- 文件被占用：一律跳过并计数，**绝不杀句柄 / 结束进程**
- 无定时任务、无后台监控；将来加定时是纯增量（timer 接线 + 设置项），架构无需预埋

## 6.5 前端页面设计流程（强制步骤）

1. **设计实施**：先加载 `frontend-styles` skill（项目强制基线），再用 **`ui-ux-pro-max`** skill 做页面设计与实现（扫描结果页 / 目标勾选列表 / 隔离区页 / 置灰项展示）
2. **Mock 截图审查**：页面完成后，dev 运行态下 Chrome headless 截图，交 `vision` subagent 评审（调用时显式指定 `范围: 桌面应用内`），同时加载 `design-taste-frontend-v1` 作为品味基线
3. **按意见修改 → 复审**：评审发现的问题逐项修复后重新截图复审，直到无阻断性问题；截图与评审结论留档在 `.scratch/disk-cleaner-plugin/reviews/`
4. 未走完本流程的前端改动不得进入提交

## 7. 验收标准

### v1（Windows）原有验收项（保留）

- [ ] 零 WIT/契约改动；`wasiPreopenDirs` 唯一一项由 `${cacheRoot}` 按平台解析（v1 = `%USERPROFILE%/AppData/Local`）
- [ ] 插件代码中不存在绕过隔离区的删除路径（code review 专项检查项）
- [ ] 黑名单单测覆盖：构造同时命中规则模式与黑名单的路径，断言强制跳过
- [ ] 批次还原单测：还原后原路径内容恢复、meta 更新；原路径冲突时留隔离区
- [ ] 过期惰性清理单测：7 天前批次在 activate / 扫描时被清、未过期保留
- [ ] 占用文件场景（真实锁一个文件）扫描→清理不报错、计数正确
- [ ] i18n key 同步 zh-CN 与 en
- [ ] 插件 `cargo test` + 宿主 `cargo test --lib` + 桌面端 `pnpm run test:run` 全绿
- [ ] 前端 UI 通过 `frontend-styles` skill 自查
- [ ] §6.5 截图审查闭环完成：vision 评审无阻断问题，截图与结论已留档 `.scratch/disk-cleaner-plugin/reviews/`

### v1.1（Linux 适配）追加验收项

- [ ] 桌面端 `pnpm run tauri:build` 在 Linux（Debian/Ubuntu 22.04+）上能出 `.deb`
- [ ] `.deb` 安装后激活磁盘清理插件，扫描 `$XDG_CACHE_HOME` 命中若干项，真实清理演示通过（move 入 `~/.bedcode/cleaner-quarantine/`，还原链路验证）
- [ ] 平台分发单测：mock `target_os = "linux"` 时 `wasiPreopenDirs` 解析为 `$XDG_CACHE_HOME`；mock `windows` 时解析为 `%USERPROFILE%/AppData/Local`；不允许运行时探测
- [ ] `XDG_CACHE_HOME` 环境变量覆盖：测试设置 `XDG_CACHE_HOME=/tmp/custom-cache` 时插件识别到的根路径同步改变（不写死 `~/.cache`）
- [ ] Linux 黑名单单测：构造同时命中规则模式与黑名单的路径（如 `chromium/Default/Cookies`），断言强制跳过
- [ ] 隔离区惰性过期（Linux 路径与 Windows 行为一致）
- [ ] 桌面端 bundle 配置加 Linux deb target，`bundle.linux.deb.depends` 列 webkit2gtk-4.1 等 Tauri 2 Linux 标准系统依赖
- [ ] CI / `AGENTS.md` 构建命令字眼补充 Linux 构建入口
- [ ] 不做 AppImage（发行形态只承诺 .deb）
- [ ] §6.5 截图审查闭环（Linux 桌面端）：扫描结果页 / 目标勾选列表 / 隔离区页 / 置灰项展示，vision 评审无阻断问题

## 8. 二期预告（不实施，仅立约束）

注册表清理：仅限卸载残留 / 无效项等孤儿条目；删除前必须经 host-process 导出 `.reg` 备份入隔离区机制（同一套批次还原）；提权与否届时单独评审 ADR。启动项管理视二期余力再议。
