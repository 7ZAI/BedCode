# 10: 文档与记账

**What to build:** 把所有票落地的架构结论固化进仓库文档，保证后续 agent / 读者不会把「业务已下沉、宿主只剩引擎」误读成「业务在宿主」。逐项：ADR 0022 补记（crypto 引擎裁剪线 / host-crypto / enums 三分类处置 / 协商套件参数化）；AGENTS §5（无业务内核口径更新：宿主只提供最基础 POSIX 级 API，例外仅 WASM 性能与边界）、§7（host-crypto 原语与权限位登记）、§8（crypto 原语不作为认证旁路）；code-map（`enums/` 终态分类、`crypto/` 模块、WS 声明式路由）；CHANGELOG；路线图阶段 4 状态推进；移动端受损清单（OW 桌面专属登记 + 受影响 wire 形状）。

**Blocked by:** 02、03、04、05、06、07、08、09c 全部完成

**Status:** ready-for-agent

- [ ] ADR 0022 补记（crypto 引擎化 + host-crypto 裁剪线 + enums 三分类处置）
- [ ] AGENTS §5/§7/§8 同步（宿主解耦原则、host-crypto 权限位、crypto 非旁路声明）
- [ ] code-map 更新（`enums/` 终态分类、`crypto/` 模块、WS 声明式路由）
- [ ] CHANGELOG + 路线图阶段 4 状态推进 + 移动端受损清单（含 host-crypto 桌面独有偏离）