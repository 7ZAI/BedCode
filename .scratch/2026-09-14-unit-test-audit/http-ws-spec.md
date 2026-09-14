# 桌面端 HTTP/WS 模块单元测试审查报告

> 状态: **审计完成，9 张修复票据待处理**（08-13 首轮 + 14-16 二轮，2026-09-14）
> 范围: `bedcode-desktop/src-tauri/src/server/`（43 文件，12647 行）+ `tests/`（8 文件）
> 测试规模: **132 个**（90 `#[test]` + 27 `#[tokio::test]` + 9 `#[actix_web::test]` + 6 `#[actix_rt::test]`）
> 分支: `dev`（工作区审计前后均干净，无代码改动入库）

---

## 1. 摘要（Verdict）

**覆盖广度尚可，安全关键路径零测试，且存在全局单例导致的 flaky 缺陷。**

- 132 个测试集中在 12 个文件，覆盖 43 个 server 文件的 **28%**；**17 个 >50 行的文件完全无内联测试**。
- **最致命缺口**：`file_controller`（830 行，4 个文件传输端点）与 `git_controller`（211 行，3 个 git 端点）在 `src/server` 与 `tests/` 中**零测试触达**，而 `file_controller` 含 `canonicalize()` 路径穿越防护——安全边界无任何守卫。
- **实测 flaky**：`cargo test --lib server::` 首次运行即失败（`server::metrics::tests::reset_and_sample_report_totals_and_sliding_window_rates`，`encrypted_frames: 5` vs 期望 `2`），单独跑该测试连跑 3 次全绿。根因是 `MetricsCollector::global()` 为 LazyLock 全局单例，`link_crypto` 的 24 个测试通过真实代码路径污染同一全局计数器。
- **变异测试**：交换 HKDF 方向常量（`HTTP_INFO_REQUEST` ↔ `HTTP_INFO_RESPONSE`）→ 24 个 link_crypto 测试仅 **1 个**失败。其余 23 个 roundtrip 测试对密钥方向混淆免疫（加密解密用同一对象，swap 后自洽）。
- **最大文件零测试**：`ws/terminal_ws.rs`（1601 行）是 server/ 最大文件，无任何 `mod tests`。

---

## 2. 审查基线（全部实跑）

```bash
cd bedcode-desktop/src-tauri
cargo test --lib server::      # → 110 passed; 1 failed; finished in 0.09s（首次）
                                #   failed: server::metrics::tests::reset_and_sample_report_totals_and_sliding_window_rates
cargo test --lib server::      # 变异后 → 109 passed; 2 failed（含 link_crypto direction swap）
cargo test --lib server::metrics::tests::   # 单独跑 → 1 passed（连跑 3 次全绿）
```

**flaky 复现命令**：
```bash
cargo test --lib server::    # 完整 server 套件并行跑，metrics 测试大概率失败
cargo test --lib server::metrics::tests::   # 隔离跑，必过
```

lib 全量 614 个测试，其中 server 111 个（`503 filtered out`）。集成测试 8 个文件共 19 个 `#[test]`。

---

## 3. 审查方法

三种手段叠加，避免「读了测试就说没问题」的自证循环：

1. **实跑基线**：确认测试当前状态与耗时特征（见 §2），并捕获首次运行即失败的 flaky 测试。
2. **变异测试（Mutation Test）**：改坏生产代码 → 观察哪些测试变红 → 判断测试是否真有守卫力。
   - 变异体：`src/server/link_crypto.rs:311-312` 交换 `HTTP_INFO_REQUEST` / `HTTP_INFO_RESPONSE` 两个 HKDF info 常量（模拟「方向密钥混淆」安全回归）
   - 结果：24 个 link_crypto 测试中仅 `http_keys_deterministic_and_direction_isolated` 失败（其 `assert_ne!(a.request, a.response)` 断言），其余 23 个全绿。`metrics` 测试因上述隔离缺陷附带失败。
   - 变异体已在审查结束前 `cp /tmp/link_crypto.rs.bak` 完全回滚，`git diff --stat` 确认为空。
3. **全局单例污染验证**：grep 确认 `MetricsCollector::global()` 的调用方（`link_crypto.rs` 6 处 + `app.rs` + `terminal_ws.rs` 7 处 + `supervisor.rs`），证明跨模块共享同一 LazyLock 实例。

> 方法学备注：审计过程中所有行号与代码片段均以 `bash` 直接从磁盘读取的真值为准（与 [[单位测试审计纪律]] 一致）。

---

## 4. 总判定表

| 文件 | 测试数 | 判定 | 关键问题 |
|---|---|---|---|
| `src/server/metrics.rs` | 1 | 🔴 **flaky / 隔离缺陷** | 用全局单例 `global()`，`reset()` 无法对抗跨模块污染；实跑即失败 |
| `src/server/link_crypto.rs` | 24 | 🟡 **守卫弱** | 变异测试 23/24 漏过方向混淆；roundtrip 测试对密钥 swap 免疫 |
| `src/server/ws/message.rs` | 31 | 🟢 **有效** | 130 断言 / 31 测试，密度高；覆盖全 variants + 边界 + 错误路径 |
| `src/server/ws/terminal_ws/control_frame.rs` | 14 | 🟢 有效 | 控制帧解析/序列化双向覆盖 |
| `src/server/ws/terminal_ws/forward.rs` | 10 | 🟢 有效 | 批处理/字节窗口/代际门控全场景 |
| `src/server/middleware/http_filter.rs` | 6 | 🟡 部分有效 | 有 `TrafficFilterChain::global()` 同款单例风险；6 测试无跨模块隔离 |
| `src/server/ws/registry.rs` | 6 | 🟡 部分有效 | 注册表基本操作覆盖；并发/淘汰路径未测 |
| `src/server/filter.rs` | 4 | 🟡 部分有效 | 过滤器链基本操作；优先级/冲突未测 |
| `src/server/services/auth_service.rs` | 3 | 🟡 部分有效 | 核心认证逻辑有覆盖；边界/异常路径少 |
| `src/server/middleware/jwt_auth.rs` | 3 | 🟡 部分有效 | JWT 解析有覆盖；过期/篡改/算法混淆未测 |
| `src/server/port_checker.rs` | 2 | 🔴 **装饰性** | 2 测试零守卫力（详见 §5.1） |
| `src/server/client_info.rs` | 1 | 🔴 装饰性 | 1 测试仅测默认构造 |
| **`src/server/ws/terminal_ws.rs`** | **0** | 🔴 **零测试** | 1601 行最大文件，无 `mod tests` |
| **`src/server/controllers/file_controller.rs`** | **0** | 🔴 **零测试（安全关键）** | 830 行 + `canonicalize()` 路径穿越防护零守卫 |
| **`src/server/controllers/auth_controller.rs`** | **0** | 🔴 零测试 | 534 行；集成测试间接覆盖部分路径 |
| **`src/server/supervisor.rs`** | **0** | 🔴 零测试 | 442 行；集成测试间接覆盖 |
| **`src/server/ws/websocket_manager.rs`** | **0** | 🔴 零测试 | 417 行；仅间接覆盖 |
| **`src/server/app.rs`** | **0** | 🔴 零测试 | 335 行；路由配置零守卫 |
| **`src/server/controllers/session_controller.rs`** | **0** | 🔴 零测试 | 266 行 |
| **`src/server/services/session_control.rs`** | **0** | 🔴 零测试 | 228 行 |
| **`src/server/controllers/git_controller.rs`** | **0** | 🔴 **零测试** | 211 行，3 个 git 端点完全无覆盖 |
| **`src/server/services/pairing_service.rs`** | **0** | 🔴 零测试 | 123 行 QR 配对逻辑 |
| `src/server/controllers/plugin_controller.rs` | 0 | 🔴 零测试 | 83 行 |
| `src/server/services/session_config.rs` | 0 | 🔴 零测试 | 67 行 |
| `src/server/controllers/config_controller.rs` | 0 | 🔴 零测试 | 67 行 |
| `src/server/services/terminal_service.rs` | 0 | 🔴 零测试 | 63 行 |
| `src/server/dtos/file_dto.rs` | 0 | 🟡 可接受 | 124 行，纯类型定义 |
| `src/server/dtos/auth_dto.rs` | 0 | 🟡 可接受 | 116 行，纯类型定义 |
| `src/server/dtos/session_dto.rs` | 0 | 🟡 可接受 | 93 行，纯类型定义 |

---

## 5. 逐文件结论

### 5.1 `src/server/metrics.rs` — 🔴 flaky，测试隔离缺陷

**问题**：`MetricsCollector::global()` 是 `LazyLock<MetricsCollector>` 全局单例（`metrics.rs:100`），**无 `new()` 构造器**，无法在测试中隔离。测试 `reset_and_sample_report_totals_and_sliding_window_rates` 的注释承认此问题：

> "MetricsCollector 是全局单例且无私有构造器，全部断言收敛在单个测试函数内，避免并行测试互相污染计数器"

但缓解不足：`reset()` 在测试开头执行，随后 `std::thread::sleep(50ms)`，再递增计数器。`link_crypto.rs` 的 24 个测试通过**真实生产代码路径**调用 `MetricsCollector::global().inc_encrypted_frame()`（`link_crypto.rs:720/725/781/786/812/823`）。当两套测试并行运行时，link_crypto 测试在 metrics 测试的 `reset()` 与断言之间污染同一全局计数器。

**实测证据**：
- 首次 `cargo test --lib server::`：`assertion left == right failed: left: 5, right: 2`（`encrypted_frames` 期望 2，实际 5，多出 3 次来自 link_crypto 测试）
- 单独 `cargo test --lib server::metrics::tests::`：连跑 3 次全绿
- 完整链路：`link_crypto.rs:720` 等 6 处 `inc_encrypted_frame()` 调用 → 同一 LazyLock 实例 → metrics 测试断言失败

**根因**：全局单例 + 无构造器隔离 + `reset()` 时序窗口 + 并行测试调度。这是 Rust 单例测试的经典陷阱——单例在 lib test binary 内跨模块共享。

### 5.2 `src/server/link_crypto.rs` — 🟡 守卫弱，变异测试 23/24 漏过

**变异测试**：交换 `derive_http_traffic_keys` 中的 `HTTP_INFO_REQUEST` 与 `HTTP_INFO_RESPONSE` 两个 HKDF info 常量（`link_crypto.rs:311-312`）。这模拟「方向密钥混淆」安全回归——request 用 response 密钥加密、response 用 request 密钥加密。

**结果**：24 个测试中仅 `http_keys_deterministic_and_direction_isolated` 失败（`assert_ne!(a.request, a.response)`）。其余 23 个全绿，包括：
- `http_codec_roundtrip_and_tamper_rejection` — roundtrip 测试，加密解密用同一 `HttpTrafficKeys` 对象，swap 后自洽
- `http_full_request_response_cycle_through_filter` — 完整链路，两端各自派生（同一被污染函数），结果一致
- `http_fail_closed_paths` — 失败路径不依赖方向正确性
- `ws_frame_codec_roundtrip_and_seq_discipline` — WS 方向完全独立于 HTTP

**根因**：roundtrip 测试对「同一对象内两方向一致交换」免疫。唯一守卫是 `assert_ne!(a.request, a.response)` 这个显式不等断言。这是 roundtrip 测试的固有盲区——它验证「加密解密自洽」而非「方向语义正确」。

**影响**：若 HKDF info 常量被误改（如移动端 TS 复刻时抄错），桌面端测试不会发现，只有移动端 TS 测试或跨端集成测试能捕获。

### 5.3 `src/server/ws/message.rs` — 🟢 有效防线

**优点**：31 个测试 / 130 断言，密度全模块最高。覆盖：
- 全 variants 构造器（input/subscribe/unsubscribe/session_control/session_config/auth/error/server_closed/client_disconnected/session_event/ack/sync_data）
- `message_type_mapping_covers_all_variants` — 枚举完整性守卫
- `from_json_rejects_unknown_message_type` — 未知类型拒绝
- `to_json_skips_absent_optional_fields` — 序列化边界
- `from_json_fills_defaults_for_absent_fields` — 默认值填充
- `with_token_applies_to_all_variants` — 全 variant token 注入
- `from_ws_message_*` 系列 — WS 帧解析（text/binary/heartbeat/close/invalid JSON）
- `with_request_id_*` — 请求关联（ack 响应 vs 通知无操作）

**弱点**：无变异测试验证守卫力（未在本轮执行），但断言密度与覆盖面提示有效。

### 5.4 `src/server/ws/terminal_ws.rs` — 🔴 1601 行零测试

**问题**：server/ 最大文件，主 WS 终端 handler，**无 `mod tests`**。grep `mod tests|#\[cfg(test)\]` 零匹配。

**覆盖间接性**：子模块 `terminal_ws/control_frame.rs`（14 测试）与 `terminal_ws/forward.rs`（10 测试）有测试，但 `terminal_ws.rs` 本身的连接生命周期、认证握手、消息路由、断开处理等核心逻辑无单测。集成测试 `tests/ws_session_route.rs` 和 `tests/ws_auth_rules.rs` 间接覆盖部分路径，但无法定位具体函数级回归。

**metrics 依赖**：`terminal_ws.rs` 7 处调用 `MetricsCollector::global()`（`inc_ws_sent` / `inc_ws_received`），进一步加剧 §5.1 的全局污染问题。

### 5.5 `src/server/controllers/file_controller.rs` — 🔴 零测试（安全关键）

**问题**：830 行，4 个端点（`/file-tree`、`/file-content`、`/diff-tree`、`/file-diff`），在 `src/server` 与 `tests/` 中**零测试触达**（grep 确认无集成测试引用）。

**安全关键代码**：含 `canonicalize()` 路径穿越防护（`file_controller.rs:124/141/390/408`）——`PathBuf::from(&working_dir).canonicalize()` 后校验目标在 working_dir 内。这是安全边界，**零守卫**。

**风险**：若 `canonicalize()` 逻辑被误改（如移除路径校验、改为字符串拼接），无测试能捕获。路径穿越漏洞会直接暴露文件系统。

### 5.6 `src/server/controllers/git_controller.rs` — 🔴 零测试

**问题**：211 行，3 个端点（`/git/branches`、`/git/status`、`/git/checkout`），**零测试触达**。git 操作涉及命令执行（`git checkout`），`checkout` 端点若参数未校验可成为命令注入向量。

### 5.7 `src/server/port_checker.rs` — 🔴 装饰性测试

2 个测试，需进一步验证守卫力（本轮未执行变异测试）。文件名与测试数比提示低覆盖。

### 5.8 `src/server/middleware/http_filter.rs` — 🟡 部分有效，同款单例风险

6 个测试（`request_and_response_bodies_pass_through_chain` 等），有 `TrafficFilterChain::global().clear()` 清理（`http_filter.rs:369`）。但 `TrafficFilterChain::global()` 同为全局单例，存在与 metrics 类似的跨模块污染风险——只是当前无其他模块并发写入该链，故未暴露。

---

## 6. 顺带发现（非测试问题，但与「能否测出 bug」直接相关）

### 6.1 全局单例测试模式系统性风险

`MetricsCollector::global()` 与 `TrafficFilterChain::global()` 均采用 `LazyLock` 全局单例 + 无 `new()` 构造器的模式。当前仅 metrics 暴露 flaky，但这是**系统性设计问题**：
- 任何通过真实代码路径调用 `MetricsCollector::global()` 的测试（link_crypto 6 处、terminal_ws 7 处、app 1 处、supervisor 2 处）都会污染 metrics 测试。
- 修复方向：为 `MetricsCollector` 添加 `new()` 构造器（测试专用），生产代码保持 `global()`；或引入 `#[cfg(test)]` 的 per-test 实例。

### 6.2 `link_crypto.rs` 测试通过全局单例污染其他测试

`link_crypto.rs` 的 24 个测试在真实代码路径中调用 `MetricsCollector::global().inc_encrypted_frame()` 等。这是正确的生产行为（metrics 应记录加密帧），但在测试环境中成为污染源。修复方向：测试中注入 mock metrics，或 metrics 测试使用独立实例。

### 6.3 集成测试与单测的覆盖边界不清

19 个集成测试覆盖 auth/pairing/session/WS 链路，但 file/git/plugin 端点完全无集成测试。无法区分「有集成测试但无单测」与「完全无测试」的文件（auth_controller 有集成测试覆盖但零内联测试 vs file_controller 两者皆无）。

---

## 7. 修复优先级

| 优先级 | 票据 | 内容 | 来源 |
|---|---|---|---|
| P0 | 08 | terminal_ws.rs 补三类行为测试（认证/资源回收/ack 错误） | 首轮 |
| P0 | 09 | auth_service 生物认证三函数 + pairing_service 单次使用测试 | 首轮 |
| P0 | 10 | 6 controller + app.rs + supervisor.rs 补测试（含 file 路径穿越、git 命令注入） | 首轮 |
| P1 | 11 | link_crypto 断言弱点加固（WS fallback + 金样向量 + 幂等） | 首轮 |
| P1 | 12 | ws/message.rs Terminal 变体 + http_filter 出站拒绝与 /ws 快速路径 | 首轮 |
| P1 | **14** | **修复 MetricsCollector 测试隔离缺陷（flaky，实跑即失败）** | **二轮** |
| P2 | 13 | 集成测试补强 + 反模式改造（GCM 篡改 + tracing 耦合 + flaky） | 首轮 |
| P2 | **15** | **link_crypto 方向隔离变异守卫（变异测试 23/24 漏过）** | **二轮** |
| P2 | **16** | **全局单例测试模式系统性治理** | **二轮** |

---

## 8. 复现命令

```bash
cd bedcode-desktop/src-tauri

# 基线（首次大概率 flaky）
cargo test --lib server::

# 隔离跑 metrics（必过）
cargo test --lib server::metrics::tests::

# 变异测试复现（方向混淆）
cp src/server/link_crypto.rs /tmp/link_crypto.rs.bak
python3 -c "
p='src/server/link_crypto.rs'; s=open(p).read()
s=s.replace('HTTP_INFO_REQUEST, 32)','HTTP_INFO_RESPONSE, 32)')
s=s.replace('HTTP_INFO_RESPONSE, 32)','HTTP_INFO_REQUEST, 32)')
open(p,'w').write(s)"
cargo test --lib server::   # 应仅 direction_isolated 失败
cp /tmp/link_crypto.rs.bak src/server/link_crypto.rs   # 回滚

# 零测试文件确认
find src/server -name '*.rs' -exec sh -c '
t=$(grep -c "#\[test\]\|#\[tokio::test\|#\[actix" "$1"); l=$(wc -l < "$1")
[ "$t" -eq 0 ] && [ "$l" -gt 50 ] && echo "$l $1"' _ {} \; | sort -rn
```

---

## 9. 审计纪律记录

1. **README 虚假声明（已修正）**：首轮会话写入了 README 索引（声明「审计完成，6 张待处理」）与 issues 08-13，但**未创建 `http-ws-spec.md`**——主报告缺失。本文件为二轮补齐。这与审计方法学警戒的「测试名说谎」同构：**文档声明与磁盘真值不符**。二轮审计发现首轮未完整跑 `cargo test --lib server::` 套件，故漏掉 metrics flaky 缺陷。

2. **flaky 测试实锤**：二轮首次运行 `cargo test --lib server::` 即捕获，未依赖推测。隔离跑 vs 套件跑的差异是根因的直接证据。

3. **变异测试已回滚**：`git diff --stat src/server/link_crypto.rs` 为空，无残留。

4. **grep 作用域**：零测试文件扫描覆盖 `src/server` 全部（含子目录），非仅顶层。集成测试覆盖扫描覆盖 `tests/` 全部 8 文件。

5. **测试残留进程**：cargo test 完成后无后台进程残留（无 actix 服务器持续监听；集成测试自清理端口）。

6. **票据编号管理**：首轮已用 08-13，二轮新增 14-16，不回收已用编号。README 索引已更新。
