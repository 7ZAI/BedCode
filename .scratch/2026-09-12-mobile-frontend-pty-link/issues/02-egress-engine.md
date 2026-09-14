# 02 — Egress Policy 引擎 `egress.rs`（L1/L2/L3 判定 + 授权记忆 + 弹窗桥 Rust 侧）

**What to build:** 新增 `src-tauri/src/egress.rs`：外网访问三层校验（L1 桌面端目标放行 → L2 静态声明（宿主内置 + 插件 preauthUrls）→ L3 授权弹窗（懒触发，事件到前端渲染，结果回 Rust 裁决）），fail-closed。裁决与记忆在 Rust 端（§8 安全红线），授权记忆存 Rust 持久层（会话级默认 + 可选持久），不落 localStorage（spec §5.6 机制要点 1/3/4、§9 D7/D8）。

**Spec:** §5.6（Egress Policy 全部）、§9 D7（会话级默认 + 可选持久）、D8（请求时懒触发 + 设置页可查看/撤销）

**Blocked by:**

**Status:** done

## 关键实现事实（handoff §2/§3 已核实）

- L1 判定源：`ConnectionManager.target: Arc<RwLock<Option<TargetDevice{address,port,name}>>>`；**httpProbe 在 ws_connect 前调用（target 未设）**——L1 判定的「目标 host:port」集合需包含「最近一次 ws_connect/probe 目标」，具体方案在 ticket 03 落（http_request 加 kind 参数），本 ticket 的 L1 判定接口按该形态设计（`is_desktop_target(host, port)` 函数，集合由 connection 模块注入）。
- 借鉴 fs_auth 三层（路径白名单 → 插件白名单 → 弹窗授权，AGENTS.md §7）；弹窗桥借鉴 file-transfer ConsentDialog 模式（来源展示 + 确认/拒绝）。
- 错误码：`EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED`（fail-closed，请求不发）。
- 授权记忆粒度：单次 / 会话（默认）/ 持久（带「不再询问」勾选，§9 D7）；存储参考现有 settings JSON 文件惯例（宿主 Rust 持久层，非 localStorage）。

## 实现清单

- [x] `egress.rs`：`EgressDecision`（Allow / NeedConsent / Deny）判定接口 + L1（目标 host:port 集合）/ L2（宿主内置 + 插件 preauthUrls 声明合并）/ L3（授权记忆命中）判定
- [x] 授权记忆：会话级 `HashMap<host, pattern>` + 持久层（JSON 文件或 SQLite，参考既有 settings 存储惯例；记录按 host 级 + 可选 path 模式）
- [x] L3 弹窗桥 Rust 侧：emit `egress_consent_request` 事件（含域名/路径/来源插件或调用方、request_id）→ 前端渲染 → invoke 结果回 Rust 裁决（oneshot channel，需超时兜底，超时视为拒绝）
- [x] `EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED` 错误码定义与映射
- [x] 单测：L1/L2/L3 判定矩阵、记忆粒度（单次/会话/持久）、错误码、弹窗桥超时兜底
- [x] `lib.rs` 注册 egress 模块

## 验证

- `cargo test`（src-tauri）egress 相关用例全绿
- L2 声明合并接口可供 ticket 05（插件 preauthUrls 收集）与 ticket 03（http_request 校验）调用

## Comments

- 2026-09-11 完成：`src/egress.rs`（EgressPolicy 全局单例：L1 桌面目标集合 `add/clear_desktop_target`，L2 宿主内置常量（`system/constants/egress.rs` GitHub API）+ 插件声明 `register/unregister_plugin_urls`，L3 会话/持久记忆 `egress_grants.json` + 弹窗桥 `request_consent`/`resolve_consent`（oneshot + 30s 超时视为拒绝，pending map 清理防膨胀））；`commands/egress.rs`（egress_consent_resolve / egress_list_grants / egress_revoke_grants）注册 lib.rs；AppError 加 `Egress(String)` 变体。URL 轻量解析 `parse_url_lite` + 声明模式 `UrlPattern`（`*.` 子域通配 + path 前缀 glob）。
- 坑：glob `*` 初始被当字面量存进 path_prefix（`/*` 永不匹配）——修复为解析时剥末尾 `*` 作前缀通配。
- 验证：egress 7 单测全绿（L1/L2/L3 矩阵、记忆粒度、错误码、非法 URL fail-closed）+ 全量 258 passed。
- 遗留：L1 时序接线（add_desktop_target 调用点）属 ticket 03；插件声明注册调用点属 ticket 05；弹窗 UI 属 ticket 10。
