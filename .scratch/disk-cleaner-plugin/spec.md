# 磁盘清理插件（桌面端 WASI）实施规格

Status: ready-for-agent
Date: 2026-08-26
决策来源: grilling 会话（同日），安全模型另见 `docs/adr/0023-disk-cleaner-least-privilege-quarantine.md`

## 1. 定位

桌面端内置插件（WASI 组件）：面向 C 盘个人缓存数据与用户域系统残留的磁盘清理工具，对标 CCleaner 类工具的缓存清理能力。**注册表清理不在本期范围**，二期独立立项且强制带「备份→清理→一键还原」闭环。

## 2. 架构约束（硬性）

- 纯 WASI 插件：预打开目录内用原生文件 API（`fd_readdir` / read / remove）完成枚举与删除
- **零 WIT 改动、零 ABI bump**：不新增任何宿主接口函数；目录枚举靠 wasiPreopenDirs 解决
- manifest 声明 `"wasiPreopenDirs": ["${home}/AppData/Local"]`——白名单只此一项，**禁止盘根级授权**
- 预打开挂载发生在 activate；首次授权走既有 fs_auth 弹窗流程（`request-auth` + 授权持久化）
- 删除实现为「move 到隔离区 → 过期后删除」，插件代码中**不存在绕过隔离区的直接 remove 路径**

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

### 出圈（明确不做）

- 注册表清理（二期独立立项）
- 启动项 / 开机自启管理
- 大文件分析 / 磁盘空间可视化
- 回收站清空（盘根 `$Recycle.Bin` 不在授权范围；Windows 有原生入口）

## 4. 安全模型（四层防御）

1. **白名单**：预打开仅 `{home}/AppData/Local`，能力上限
2. **规则清单**：上表即清单，UI 逐项展示 + 勾选；实际删除的唯一真源
3. **硬黑名单**（优先级高于模式命中，命中即跳过）：
   - 浏览器：`**/Cookies`、`**/Cookies-journal`、`**/Login Data*`、`**/Web Data*`、`**/Bookmarks*`、`**/History*`、`**/Favicons*`
   - 通用：各应用顶层配置文件 / 本地数据库（如 `User Data/Local State`）；规则模式不得匹配 `{home}/AppData/Local` 直接子级的非 Temp 条目
4. **隔离区**：见 §5

## 5. 隔离区与批次

- 位置：`{home}/.bedcode/cleaner-quarantine/<batchId>/…`（batchId 为时间戳派生 ID）
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

- [ ] 零 WIT/契约改动；`wasiPreopenDirs` 仅一条 `${home}/AppData/Local`
- [ ] 插件代码中不存在绕过隔离区的删除路径（code review 专项检查项）
- [ ] 黑名单单测覆盖：构造同时命中规则模式与黑名单的路径，断言强制跳过
- [ ] 批次还原单测：还原后原路径内容恢复、meta 更新；原路径冲突时留隔离区
- [ ] 过期惰性清理单测：7 天前批次在 activate / 扫描时被清、未过期保留
- [ ] 占用文件场景（真实锁一个文件）扫描→清理不报错、计数正确
- [ ] i18n key 同步 zh-CN 与 en
- [ ] 插件 `cargo test` + 宿主 `cargo test --lib` + 桌面端 `npm run test:run` 全绿
- [ ] 前端 UI 通过 `frontend-styles` skill 自查
- [ ] §6.5 截图审查闭环完成：vision 评审无阻断问题，截图与结论已留档 `.scratch/disk-cleaner-plugin/reviews/`

## 8. 二期预告（不实施，仅立约束）

注册表清理：仅限卸载残留 / 无效项等孤儿条目；删除前必须经 host-process 导出 `.reg` 备份入隔离区机制（同一套批次还原）；提权与否届时单独评审 ADR。启动项管理视二期余力再议。
