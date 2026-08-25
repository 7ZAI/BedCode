# 09 — 测试移植收尾与全量验证

**What to build:** 迁移收尾的质量关：对照被删宿主编排测试的场景矩阵逐一销案（确认已被各插件票的单测承接，或补齐遗漏场景），然后全量验证——两端前端测试套件全绿、类型检查干净、spec 验收锚点逐条过。

**Blocked by:** 08

**Status:** done

- [x] 场景矩阵销案表：宿主四个编排测试文件的每个用例 → 承接它的插件侧单测（01–04 产出）一一对应；发现遗漏场景在本票补齐
- [x] 补齐迁移规则匹配纯函数与撤销信任流程的显式单测（若前序票未覆盖）
- [x] 两端 `npm run test:run` 全量绿（注意 vitest watch 陷阱：必须 run 模式）
- [x] 两端前端类型检查干净（vue-tsc 无错误）
- [x] spec「Further Notes 验收锚点」三条逐条核过并在票内记录结果：宿主无对等 UI 残留 / 测试全绿 / 构建产物同步后冒烟三项核心流可见
- [x] 真机遗留项如实记录到票内 Comments（不阻塞本票关闭）：多设备互见、后台在线状态等需真机环境验证的点

## Comments

### 前序票发现移交（2026-08-25，07 执行时记录）

- **桌面 src-tauri 集成测试编译失败（既有债）**：切收口提交 20c2b81c 退役旧文件服务后，4 个测试文件仍引用已删除的 `AppContextBuilder.file_service()`（ws_auth_rules.rs / pty_session_chain.rs / ws_session_route.rs / http_auth_biometric.rs）；另有个别测试存在 `AppError` 未实现 Display 的编译错。cargo check --tests 当前不过，08/09 动宿主时需一并修复；与本会话改动无关（07 未触 src-tauri）。
- **桌面插件 manifest 陈旧命令登记**：contributes.commands 中 `set-concurrency`、`sweep-intents` 在 lib.rs 已无对应处理（unknown command），属可顺手清理项；移动端 `remove-task` 登记但代理按「宿主托管生命周期」显式报 unsupported（有意保留入口拒绝）。

### 执行记录（2026-08-25）

#### 一、场景矩阵销案表

被删宿主编排测试共 9 个文件（桌面 5 / 移动 4，spec 决策 10/11 所指「四个」为移动侧清单）。逐用例销案如下：

**桌面 usePeerConsent.test.ts（10 用例）**

| 宿主用例 | 销案去向 |
|---|---|
| matchesTerminalPairedDevice 精确名匹配 / 无名不匹配（×2） | **按设计不迁移**——spec 决策 6 桌面首连一律人工确认，迁移规则为移动专属（决策 7）；移动端承接见下文 mobile useConsent |
| unknown device 弹窗 + accept 结算 | ✅ ticket 03 `desktop useConsent.test.ts`（presents-first + accept-routes） |
| deny 关闭弹窗并应答 false | ✅ 同上（deny routes accepted:false） |
| 并发请求单闸门排队 | ✅ 同上（presents first request and queues） |
| 终端配对设备静默自动互信 | **按设计不迁移**（同上） |
| 配对名单加载失败回退人工弹窗 | **按设计不迁移**（桌面无名单读取路径） |
| 30s 无操作自动拒绝 | ✅ 同上（timeout settles as rejection first） |
| 弹窗占用时排队配对设备自动互信 | **按设计不迁移** |

**桌面 usePeerDevices.test.ts（11 用例）→ ticket 02 `desktop usePeerDevices.test.ts`（14 例）+ `deriveDeviceRows.test.ts`（7 例）全承接**：start 快照、事件整表替换、连接成功态、denied/unreachable 行内错误、拒拨不可发现/无能力节点、并发重复拨号防抖、断连摘除徽标、断开乐观更新、连接成功清错误反馈一一对应；另新增 connOnline 与 peer 连接态分离、switchPeer 路由、active offline fallback、行派生纯函数覆盖。

**桌面 usePeerReceiving.test.ts（10 用例）**

| 宿主用例 | 销案去向 |
|---|---|
| offerDeadline / remainingSeconds / pickCurrentOffer 纯函数（×3） | **随 v2 宿主托管模型消亡**：倒计时由宿主 sweeper 托管、当前批由 pending 批卡片直接呈现，前端不再自算 |
| start 注册监听 + 初始快照 | ✅ **本票补** `useReceiving.test.ts` refresh pulls three lists |
| receive-changed 整表替换 | ✅ 本票补 snapshot events replace wholesale |
| respond 转发 accepted 标志 | ✅ 本票补 approve/reject route commands |
| cancel 路由 | ✅ 本票补 cancel-receiving routes command |
| setPolicy 后端校验后本地生效 | ✅ 本票补 `useSettings.test.ts` setReceivingPolicy |
| setDownloadDir 转发路径（桌面） | ✅ 本票补 pickDownloadDir applied/cancelled/failure 三分支 |
| clearHistory 返回移除数 | ✅ 本票补命令路由；返回值语义随 v2 整表清空消亡 |

**桌面 usePeerRemoteFiles.test.ts（13 用例）**

| 宿主用例 | 销案去向 |
|---|---|
| joinRel / sumSelectedBytes / formatBytes / toggleAllFiles 纯函数（×4） | joinRel **随 dirId 两级契约消亡**（客户端不再拼路径）；toggleAll files-only ✅ 本票补；selectedTotalSize 派生 ✅ 本票补（mobile selectedTotalSize 用例）；formatBytes 保留于 utils/format 由组件消费（无行为变更，不单列） |
| enumerateDirFiles BFS 扁平化 + too-many-files 上限（×2） | **随两级契约演进消亡**：逐层浏览取代客户端扁平枚举，上限保护回归宿主 browse_directory |
| open 单根自动进入 / 多根停留选择器 / 零根空选择器不报错（×3） | ✅ 本票补 `useRemoteFs.test.ts`（enterRoot / loadRoots chooser / zero roots keeps chooser no error） |
| enterDir/navigateTo 面包屑栈 | ✅ 本票补 cd/up/goTo/goRoot breadcrumb 断言 |
| pullSelection 入队所选文件（含目录枚举） | **按设计不迁移**（spec 决策 13 不重引入扇出编排）；远端拉取发送以活跃对端粒度由 `useTasks` sendPickedFiles enqueue 承接 ✅ 本票补 |
| filtered notice 仅空过滤列表显示 | ✅ 本票补 notice passthrough 用例 |
| browse 失败错误键不污染状态 | ✅ 本票补（含失败后恢复） |
| roots 拉取失败错误键 | ✅ 本票补；key 名随 i18n 归位为 `transfer.table.dirUnavailable` |

**桌面 usePeerTransfers.test.ts（9 用例）→ 本票补 `useTasks.test.ts`（9 例）**：start 注册+初始快照、start 幂等、transfer-changed 全量替换（进度+终态并存）、cancel/retry 命令路由、空选拒绝不发命令均承接；sendToPeers 逐节点扇出/失败独立 **按设计不迁移**（决策 13，发送收敛活跃对端粒度，扇出能力服务层仍支持留后续票）。

**移动端 4 个文件（39 用例）与桌面同构映射**：

- usePeerDevices（11）/ usePeerReceiving（8）/ usePeerRemoteFiles（13）/ usePeerTransfers（9）→ 前序 `usePeerDevices.test.ts`（16 例）承接设备域；接收/远端文件/发送三域同样为**本票补齐缺口**，落地 `mobile useTasks.test.ts`（11 例）/ `useRemoteFs.test.ts`（10 例）/ `useSettings.test.ts`（6 例），销案口径与桌面一致（纯函数消亡/按设计不迁移各条相同）
- 桌面 usePeerConsent 的移动端对应物（迁移规则 + 插件对话框全局确认）已由 ticket 04 `mobile useConsent.test.ts`（19 例）完整承接：matchesTerminalPairedDevice / normalizePairedNames 纯函数直测、自动互信含弹窗占用时插队、名单损坏回退弹窗路径、30s 超时先行结算、迟到应答幂等、无名请求宁多弹勿误信

#### 二、「迁移规则纯函数 + 撤销信任显式单测」核查

- 迁移规则匹配纯函数：✅ 已覆盖（ticket 04，mobile `consent pure rules` describe：命中条件、畸形名单容忍、display name 兜底、无名不误信）
- 撤销信任流程：✅ 已覆盖（ticket 05，两端 `useTrustedPeers.test.ts`：撤销命令路由+本地摘除、撤销失败保留条目并上报、加载失败错误态、失败后重试恢复）
- 结论：该复选项无需新增代码，前序票已闭环

#### 三、全量验证结果

| 项 | 桌面 | 移动 |
|---|---|---|
| 新增测试 | 4 文件 32 例（useTasks 9 / useReceiving 7 / useRemoteFs 10 / useSettings 6） | 3 文件 27 例（useTasks 11 / useRemoteFs 10 / useSettings 6） |
| `test:run` 全量 | 57 文件 / 514 用例全绿* | 30 文件 / 295 用例全绿 |
| `vue-tsc --noEmit` | exit 0 | exit 0 |

\* 本机默认 pool 触发已知 worker OOM（56/57 文件通过 + 1 ERR_WORKER_OUT_OF_MEMORY，与 xterm 渲染优化会话记录的环境 flakiness 一致，非回归）；`vitest run --pool=forks` 全绿。

#### 四、spec 验收锚点核验（三条）

1. **宿主无对等 UI 残留 ✅**：rg 两端宿主源码树，仅命中两类预期项——`src/__tests__/plugins/file-transfer/`（插件侧承接测试本体所在）与 `src-tauri/src/peer_net.rs`（服务层，spec 明确保留）；页面组件/弹窗宿主/编排 composable/路由注册/侧边栏菜单/设置分组/i18n `peers` 命名空间均无残留。
2. **测试全绿 ✅**：见上表（桌面 514 / 移动 295，vue-tsc 双零退出）。
3. **构建产物同步后冒烟三项核心流可见 ✅**：两端插件 `dist/index.js` 与打包资源 `src-tauri/resources/plugins/{desktop,mobile}/com.bedcode.file-transfer/index.js` md5 一致（desktop `840b9cd5…` / mobile `e8c0060a…`），配套 wasm/plugin.json 同步；冒烟证据 = ticket 06 dev-shell 截图评审（两端 × 设备列表/首连弹窗/可信对端管理逐状态截图矩阵 + vision 评审结论，UI 已冻结）。

#### 五、真机遗留项（不阻塞关闭）

- **多设备互见 / 后台在线状态**：移动端 WS 被系统杀死后的重新认证与 announce 补发可靠性仍需真机联调（scratchpad 01a01f2b 遗留，弱网/多设备 checklist 属 spec §10 待执行项）
- **桌面 src-tauri 集成测试编译债**（上文移交项）：本票未触 src-tauri，未处理；建议开独立还债票
- **manifest 陈旧命令登记**（desktop `set-concurrency` / `sweep-intents`）：可顺手清理项，未动
- **移动 consent 倒计时为静态文案提示**（ticket 06 已知豁免）：真实秒级倒计时待定夺是否补
