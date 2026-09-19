# 09: consent 模块 + 互调 API（B4）

**What to build:** peer 首连确认决策能力 + 互调 API 声明（`auth.decide-consent` / `auth.list-trusted-devices`），供 file-transfer 消费（票 10）；ADR 0017「未声明 api 不可调」验证。

**Blocked by:** 08

**Status:** done（2026-09-19；验证：插件 56/56、宏 6/6、宿主 989/0 全绿）

- [x] consent 决策单测（正反例）全绿：允许 / 拒绝 / 已信任免确认 / 一次性确认
- [x] 互调闭环测试：消费方经互调调用 `auth.decide-consent` 成功返回；未声明 api 的调用被拒（ADR 0017）
- [x] api 已在 manifest `api` 字段声明（声明即契约）

**实施记录（2026-09-19）：**

- `rust/src/consent/`（model/ops/mod 三文件）：决策优先级 = 已信任免确认（host-peer
  `list-trusted` node_id 集合）> 用户显式意向（accept/deny/one_time）> ask。信任状态
  不可验证（无头上下文 require_app 失败）时 fail-closed 按未知 → ask，log_warn 透出
  原因（不静默放行/阻断，与 trust.list peerError 同哲学）。one_time = 本请求放行、
  不改变信任状态（reason 判别，消费方据此不落信任）。
- 互调 api（`#[api(...)]` 连字符覆盖名）：`auth.decide-consent`（两阶段流：阶段 1
  无意向评估信任 → ask 时弹窗；阶段 2 回传 userDecision → 最终决策）+ `auth.list-trusted-devices`
  （trust 统一视图直通）；manifest.permissions += `peer`（host-peer 可信集）。
- **宏 bug 修复（`rust-macros/src/lib.rs`）**：`#[api("decide-consent")]` 括号形态解析为
  `Meta::List`，而 `method_name_override` 只支持 `Meta::NameValue`（`api = "..."`）→
  覆盖名永远不生效（退化为 snake_case ident）；且 `parse_methods` 中 retain 先于读取剥离
  属性（第二个独立缺陷）。两处修复 + 回归测试
  `method_attr_override_participates_in_api_derivation`（真实 trait 解析路径）。
- **宿主闭环 `test_devices_consent_api_closed_loop`**：真实 devices 产物 + sdk-test caller
  双实例 → 经总线互调：阶段 1 无头 ask（fail-closed）、阶段 2 accept/explicit、deny、
  accept/one_time、list-trusted-devices 统一视图（pairing 空 + peerError 透出）、
  ghost-api 未声明被门禁拒（not declared）。
- **既有测试潜藏 bug 修复（使宿主闭环可运行）**：① `pluginType: "rust-only"` 对 SDK
  `PluginType` 枚举非法（应为 `rust`）→ wasm `manifest()` 导出必 panic（生产不可加载，
  真实阻塞 bug）；② 生命周期测试从未真正运行（`resources` 路径解析错误 → 跳过），
  断言按宿主直返 Err 写，但 `wasm_entry` 将命令 Err 包装为 `{"error": ...}` JSON →
  三处 `.is_err()` 断言全部修正为错误形状断言；③ 测试缺 manifest 权限授权（auth/storage/
  peer），host-auth 调用被权限门拒绝 → 补授权。
- 验证：插件 56/56（consent 11）、宏 6/6、宿主 **989 passed / 0 failed**（含 pty 线
  17 失败同步翻绿）、eslint 0 error。
