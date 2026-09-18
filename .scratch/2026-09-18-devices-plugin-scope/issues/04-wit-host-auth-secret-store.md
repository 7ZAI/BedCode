# 04: WIT host-auth + SDK 投影 + 桌面 secret-store 实现（A2）

**What to build:** 桌面 WIT 新增 `host-auth` 接口（secret-store 原语：secret 的 set/get/delete + 属主隔离），SDK 桌面 guest 绑定与宿主实现：属主隔离、越权拒绝、delete、重启持久化、明文不落日志、权限门 deny；ABI 桌面 14→15。**移动端不动**：AGENTS.md §7「改 WIT 双端同步」走文档化偏离（同 ws-base-service 先例，偏离记录留票 14）。

**Blocked by:** 02（接线一次到位，避免 async 化后返工）

**Status:** ready-for-agent

- [ ] secret 属主隔离：插件 A 不能读/改/删插件 B 的 secret；越权返回明确错误
- [ ] set / get / delete / 覆盖写 / 重启持久化 行为正确
- [ ] 日志与存储不落明文（只记 `token.length()` 模式）；密钥明文不出宿主
- [ ] 权限门：未声明 auth 权限的插件调用被 deny（Rust 端最终仲裁）
- [ ] 宿主单测 + SDK wasm32 check 全绿；ABI bump 14→15 后旧插件（≤14）仍可加载（`version > 当前 → 拒绝` 语义兼容）
