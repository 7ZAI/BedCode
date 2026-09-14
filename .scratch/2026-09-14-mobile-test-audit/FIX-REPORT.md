# 移动端单测修复完成报告（2026-09-15）

基于 `.scratch/mobile-test-audit/SUMMARY.md`（审核标准 unit-test-discipline G1-G6）执行的 P0/P1 修复。
三个未通过 lane（fe-utils-config 71.7 / fe-composables 79 / rust-tests 76.7）全部 P0 项落地。

---

## 1. 前端修复（fe-utils-config / fe-composables lane）

### P0

| 文件 | 问题（审核定位） | 修复 |
|---|---|---|
| `src/__tests__/composables/connectionProbe.test.ts` | 3 条恒真断言（`expect(errorMsg.includes(...)).toBe(true)` 测本地字面量，与被测代码无关） | 删除恒真断言，改为真实错误路径验证：mock invoke 触发 404/timeout/refused/unreachable 四类故障，断言 httpProbe 返回的 error 串经分类函数落入正确 toast key 白名单 |
| `src/__tests__/plugin/pluginIcon.test.ts` | SVG 消毒分支无负例（XSS 纵深防御零覆盖） | +4 条负例：`<script>` 注入、`onclick` 事件属性、`javascript:` href、`foreignObject` 注入，断言危险载荷不进入 DOM、合法 path 保留 |
| `src/__tests__/plugin/dialogHost.test.ts` | `resolveById` 完全无测（30s 超时定点结算安全关键路径） | +4 条：队中条目定点结算（不扰动队首）、未知 id 不结算、showPrompt 无 value 兜底空串、resolveById 携带 value |
| `src/services/linkCrypto.test.ts` | **测试文件位于 `src/services/` 下，被 vitest include 模式（`src/__tests__/**`）排除——从未被执行（G5 违例）**；且 encrypt-only 无 decrypt 对称验证 | **迁移到 `src/__tests__/services/linkCrypto.test.ts` 进入标准运行器**；重写为含服务端镜像（ServerWsCrypto，方向常量与桌面端 link_crypto.rs 对齐）的双向对称回环 + 全部异常分支：seq 不连续、已消费帧重放、GCM 篡改（序号不前进）、nonce 长度、错误 version、帧过短、非法 JSON |
| `src/__tests__/config/terminalOnboardingSteps.test.ts` | 恒真断言 `expect({key, value}, ...).toBeDefined()` + `if (!key) continue` 静默放行 tryHintKey | 改为强断言：必填 key 非空校验、tryHintKey 声明即必须可解析（不静默跳过） |
| `src/plugin/permission.ts` | 权限仲裁完全无测（错配即插件越权） | 新建 `src/__tests__/plugin/permission.test.ts`（12 用例）：正例命中、空列表拒绝、错配拒绝、未知权限/API 拒绝、部分前缀不命中、peer WASM-only 拒绝、大小写敏感 |

### P1

| 文件 | 修复 |
|---|---|
| `src/__tests__/composables/useTerminalScroll.test.ts` | 变异无法杀死：第二次调用传 500（原 120 恰巧等于旧值），可杀死「忘记 isAtBottom 守卫」变异 |
| `src/__tests__/composables/useAppStartup.test.ts` | 真实 `sleep(3)` → fake timers + `vi.setSystemTime` 推进 60s，幂等断言更稳 |
| `src/__tests__/integration/plugin-loader-gating.test.ts` | isEnabled=false 意图门禁（spec §3.5 核心裁决）完全未测 → 新增：启用/停用混合批次，断言 markError 仅出现在启用插件、`plugin_is_enabled` 参数契约 |
| `src/__tests__/plugins/file-transfer/usePeerDevices.test.ts` | 2.1s 真实 sleep 等待 debounce → fake timers 推进（触发新变更让定时器在 fake 时钟下重建），测试时长 2.1s→0.9s |
| `src/__tests__/utils/terminalDimensions.test.ts` | DPR=0 兜底精确断言（与 DPR=1 结果 toEqual）+ 可用宽 ≤0（容器 ≤ 滚动条预留宽）→ cols 钳制 1 边界 |

### 配套重构（错误分类契约单一真源）

- 新建 `src/utils/connectionError.ts`：`classifyConnectionError()` 纯函数，从 DevicesView 两处重复 if/else 抽取（原逻辑分散在 startConnection / connectFromScanResult，无测试、易漂移）
- `src/views/DevicesView.vue`：两处错误分类替换为调用该函数，行为零变化（分类顺序/大小写敏感语义保留）

---

## 2. Rust 修复（rust-tests lane）

### P0

| 文件 | 问题（审核定位） | 修复 |
|---|---|---|
| `src/plugin/wasm_host.rs:497-500` | `test_sanitize_plugin_id` 复制实现逻辑为预期，不引用被测代码（G4） | 删除假测试，改为 `test_validate_sql_table_prefix_sanitizes_plugin_id`：真实调用 `validate_sql_table_prefix`，消毒后前缀放行 / 未消毒表名拒绝 + 错误消息携带消毒后前缀 |
| `src/plugin/wasm_host.rs:559-562` | `test_extract_table_names_quoted` 断言 `my-table`→`"my"`，锁死正则 bug（`\w+` 不匹配 `-`） | **修真实 bug**：正则 `(\w+)` → `([^`\s"'(),;\[\]]+)` 支持 SQLite 带引号标识符连字符；断言改为完整名 `my-table`；+DML/DDL 关键字（UPDATE/DELETE/ALTER/DROP）独立覆盖、逗号分隔符边界 |
| `src/system/info.rs:121-124` | `test_local_ip_addresses` 只有 `let _ =` 无断言（G3） | 逐项断言：合法 IP 文本、仅 IPv4、非环回、非链路本地、无重复项 |
| `tests/http_proxy_flow.rs:279` | 遗留 `eprintln!("DEBUG ...")` CI 刷脏日志 | 删除 |
| `src/auth/manager.rs` | 725 行仅 1 测试，authenticate/refresh 生产路径零覆盖 | `tests/http_auth_flow.rs` +6 条 AuthManager 编排层集成测试（复用 mock actix 服务器）：request_pairing→verify 全流程（状态机/凭据/全局 token 落地）、1005 业务拒绝（Ok(false)+Failed 状态不写凭据）、reauth refresh 新 token 写回、1001 拒绝 Err、无 target 快速失败、传输故障 Failed 状态 |
| `src/plugin/manager.rs` | init 失败/权限闸门/uninstall 副作用零覆盖 | +4 条：`init_wasm_runtime` 无 app_handle 失败路径、未批准 RemoteDownload 插件激活被闸门拒绝（NeedsApproval）、已批准插件生效权限=批准∩请求（storage 恒授予）、uninstall 内置拒绝 + 非内置清理（记录/审批/目录） |
| `src/peer_net.rs` | 1915 行仅 4 测试，主链路零覆盖 | +4 条纯函数契约：`parse_node_id` 合法/空/长度不足/大写/非 hex 拒绝、`map_peer_net_error` 错误映射保留排障上下文、`bus_topic_for` 对等事件桥映射 |

### P1

- `src/system/info.rs` `test_collect_non_android`：os_name 强断言（= `std::env::consts::OS`）+ device_name 无空白/换行断言

---

## 3. 验证证据（全部实际运行）

| 验证项 | 结果 |
|---|---|
| 移动端 `pnpm run test:run` | ✅ 50 文件 / 443 用例全绿（修复前 linkCrypto 未被执行，现含 16 用例） |
| 移动端 `cargo test` | ✅ 294 lib + 17 http_auth_flow + 7 http_proxy_flow + 11 ws_protocol_integration + 1 smoke 全绿 |
| 根目录 `pnpm exec eslint .` | ✅ 0 error，119 warnings（与修复前一致的 pre-existing warning） |
| `cargo fmt --check`（改动文件） | wasm_host.rs / system/info.rs / http_auth_flow.rs 已格式化；manager.rs / peer_net.rs 存在仓库既有 fmt 漂移（rustfmt 重排 3000+ 行，按最小改动原则未混入本次改动，我的新增代码区域 fmt 干净） |
| 残留进程检查 | ✅ 无 vitest / mock server / gradle 残留进程，无监听端口 |

## 4. 发现的问题（超出审核定位）

1. **linkCrypto.test.ts 从未被执行**：位于 `src/services/`，vitest include 仅覆盖 `src/__tests__/**`。审核报告评了它却未发现其不在 CI 运行集——已迁移进标准运行器（16 用例现在真正生效）
2. **WS 方向语义澄清**：`WsSessionCrypto` 是纯客户端视角（encrypt 恒 DIR_C2S / decrypt 恒 DIR_S2C），无法直接当对端做对称测试——测试内新增 `ServerWsCrypto` 镜像（与桌面端 link_crypto.rs 出站=Outbound/S2C、入站=Inbound/C2S 逐字节一致）

## 5. 未做项（明确理由）

- `useAiChat` 早返回路径（switchConversation/regenerate/stopGeneration）：plugin-sdk lane 已通过（81.8），非本次 3 个未通过 lane 范围
- `heartbeat.rs` 时钟注入缝、`ws_protocol_integration` 多客户端并发：P2 可选，改动面大、风险高于收益
- `plugin/events.ts` / `plugin/routes.ts` 基础测试：P1，但 permission.ts（更关键的安全仲裁）已优先落地；events/routes 可排下轮
