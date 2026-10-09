//! WASM ABI 契约 — 组件模型为单一事实来源（迁移 ticket 04 / 09 清理完成）
//!
//! 组件形态下，宿主导入 / 插件导出契约全部定义在 `wit/bedcode.wit`，
//! 由 wit-bindgen 编译期校验 —— 插件侧不再需要名称常量与签名表。
//! 本模块仅保留 [`ABI_VERSION`]：插件经 WIT `abi.version()` 导出向宿主
//! 协商的版本号。core 形态遗留（自研导出/导入名常量、签名表、内存搬运常量）
//! 已随 09 清理与宿主 core 路径一并删除。

/// 当前 ABI 版本
///
/// - v1–v5：自研 ABI 演进史（组件迁移后仅作版本号序列保留）
/// - v6：组件形态（Component Model）首个版本；语义与旧 v6 一致（批量传输
///   批准协议后定稿），插件 `abi.version()` 与宿主加载校验均以本常量对齐
/// - v7: host-peer 原语化收缩第一阶段（ADR 0022 v2，issue 13）：新增
///   `dial-peer-endpoint` / `close` / `set-shared-roots` 三原语与旧函数并存；
///   新增 `host-mdns`（browse-only）与 `host-platform` 接口。纯增量变更，
///   v6 插件二进制不受影响
/// - v8: host-peer WIT 收缩 ADR 0022 v3 终态（commit 56ee094cb）：host-peer
///   移动终态 12 函数定稿（dial-peer 转正、send-files 返回传输句柄、删除
///   旧式寻址函数集）。破坏性收缩，旧插件二进制须重编译
/// - v9: host-peer 传输控制三原语（`pause-transfer` / `resume-transfer` /
///   `resume-all-transfers`），支撑暂停/恢复（issue 14）。纯增量变更，
///   v8 插件二进制不受影响
/// - v10: 消息总线二进制载荷（host-bus.publish-binary / subscribe-binary）：
///   零 JSON 编解码、可传非 UTF-8 与大载荷；新增可选导出 `events-binary`
///   （宿主实例化后动态探测，旧插件不导出则只收 JSON，不受影响）。
///   注：v10 为 dev（v9 host-peer 三原语）与本分支（总线二进制）合并后的
///   版本，宿主能力为两者超集，声明 v9 及以下的插件二进制仍可加载
/// - v11: host-mdns v2（mDNS 基础能力服务契约）：新增
///   `advertise` / `stop-advertise` / `is-advertising` 三原语 + 浏览事件
///   定向投递 `<owner>::mdns:found` / `<owner>::mdns:lost`（payload 增
///   serviceType / browserId 字段；v17 起 topic 从 `mdns:found.<owner>` 旧格式
///   统一到 `<owner>::` 属主命名空间——双端共享 lib spec M3，wire 对齐桌面
///   终态，host-mdns 函数签名不变故无 ABI 破坏）。纯增量变更，
///   v10 插件二进制不受影响
/// - v12: 发送编排下沉插件（票 06）：host-peer 删 `resume-all-transfers`
///   （批量恢复编排归插件，逐批调 `resume-transfer`）、`send-files` 收窄为
///   「一次调用即发一会话」并显性拒绝已退役的 `concurrency` 载荷字段；发送
///   方向进度/终态经新 topic `peer:transfer-event` 引擎原始事件回流（取代旧
///   快照 topic `peer:transfer`）。破坏性收缩，v11 插件二进制须重编译
/// - v13: 接收编排下沉插件（票 07）：WIT 接口函数集不变（host-peer 仍 19
///   函数），变的是**事件回流契约**——接收方向改经新 topic `peer:receive-event`
///   引擎原始事件（offer-pending / pull-started / progress / terminal / paused /
///   resumed / node-stopped），旧快照 topic `peer:receive` 退役。
///   **须重编译**：v12 插件仍只订阅 `peer:receive`，事件永不回流（接收列表空、
///   待应答弹窗不弹）——注意协商是单向的（仅拒绝高于宿主的版本），
///   低 ABI 产物不会在加载期被拒，故重编译依赖构建流程而非协商兜底
/// - v14: WebSocket 出站连接（WIT `host-websocket` **客户端域**，票 11）：新增
///   接口 5 函数（`connect` / `send-text` / `send-binary` / `close` /
///   `is-connected`）+ 权限位 `ws:client`（SSRF 面，fail-closed）+ 两条属主
///   私有投递通道（状态事件 JSON topic `<plugin-id>:ws:open|error|close`、
///   入站帧二进制 topic `<plugin-id>:ws:message`，帧信封 = kind + handle +
///   原始字节，零 JSON 编解码）。**纯增量变更**（新增接口，既有函数集不动），
///   v13 插件二进制不受影响；**不跟演桌面服务端域** 9 函数与
///   `connection-context`（移动端不跑 WS 服务器，ADR 0018/0019），
///   **不引入 `ws:server` 权限位**。
///   注：协商单向（仅拒绝高于宿主的版本），v13 产物在 v14 宿主上照常加载但
///   **没有 ws 能力**且不报错——票 12（终端迁插件）同批处置：terminal-session
///   为 v15 首发新 id（无旧产物、与宿主同 APK 分发），v15 产物在 v14 宿主
///   实例化期 import 缺失点名失败（fail-visible ②），既有 v13/v14 产物零
///   ws 需求不受影响；无需运行期能力探测（票 12 §6.4 论证）。
/// - v15: 终端订阅协议客户端迁插件（票 12）：新增
///   `host-terminal-stream.forward-output`（输出裸字节宿主零解析窄转发到
///   前端页面 Channel——C3 二进制出口，权限复用 `terminal:output`）与
///   `host-connection.primary-target`（主连接目标事实，无权限门，与桌面
///   同名不同形——C8 登记 ADR 0018 偏离表）；host-websocket config 增强
///   `jwt-auth`（宿主代发首消息认证帧，token 不落插件）/ `heartbeat-secs`
///   （连接级心跳）/ `auto-reconnect`（断线自动重连 + reconnect-scheduled
///   事件），零 WIT 形状变化。**纯增量变更**，v14 插件二进制不受影响。
/// - v16: 认证 / 配对编排下沉（票 14 阶段 B）：新增 `host-auth` 认证引擎面
///   5 函数（`request-pairing` / `verify-pairing-code` / `qr-connect` /
///   `biometric-authenticate` / `has-credentials`）+ 权限位 `auth`
///   （fail-closed）。**凭据零过境**：JWT 由宿主落地（global token + 凭据表），
///   本域不向插件返回凭据材料（对齐 v15 `jwt-auth`「token 不落插件」先例）；
///   编排（流程顺序 / 事件发射 / 状态派生）归消费插件
///   `com.bedcode.terminal-session` 配对域。**纯增量变更**，v15 插件二进制
///   不受影响；既有 v13/v14 产物无认证编排需求不受影响。
/// - v17: host-terminal / terminal-hooks 整面退役（票 15 阶段 B，终端 UI 域已
///   随阶段 A 整体迁入插件前端、退役面零消费者）：import `host-terminal`
///   （send）与导出 `terminal-hooks`（on-terminal-input / on-terminal-output）
///   删除 + 权限位 `terminal:input` 退役。**破坏性收缩**：v16 及更早产物在
///   v17 宿主实例化期因缺失 import interface 被点名失败（fail-visible ②），
///   须随 SDK 重编译；内置插件随 APK 同分发无旧产物。`terminal:output` 保留
///   （host-terminal-stream.forward-output 权限门）。
/// - v18: 通知 / 震动 / 声音整族封装（新增移动特有域 `host-notify`，5 函数：
///   notify 带 options-json 震动/声音开关 / check-permission /
///   request-permission / vibrate / play-sound）+ 权限位 `notify`
///   （fail-closed，用户打扰面独立成位）。原 `host-events.notify` 收编迁入
///   （host-events 回归纯事件语义）。**破坏性收缩**：**引用了
///   `host-events.notify` 的** v17 及更早产物在 v18 宿主实例化期因缺失
///   import 函数被点名失败（fail-visible ②），须随 SDK 重编译（组件 import
///   按实际使用面声明，未引用者不受影响）；内置插件零消费者且随 APK
///   同分发，产物随本版全量重建。
/// - v19: `host-database`（主库）整面退役（2026-10-09 双端统一机制决策）：主库是
///   wasm-core 机制内部真源（激活状态 / 审批记录 / 授权记录 / plugin_storage），
///   **不给任何 wasm-app / 插件直接调用的方法**——接口与 `database:main` 权限位
///   自双端 WIT / SDK 面移除（桌面 ABI 34→35 / 移动 18→19，破坏性）。**插件数据库
///   能力 = 插件私有库**（`host-plugin-database`，声明 `storage` 位）。引用主库
///   原语的 v18 及更早产物在实例化期因缺失 import 函数被点名失败（fail-visible ②）。

pub const ABI_VERSION: u32 = 19;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version_is_contract() {
        // 宿主加载时与组件 abi.version() 导出比对，漂移导致拒绝加载（高 ABI 拒绝测试依赖）
        // v19 = host-database（主库）整面退役（主库收归 wasm-core，双端同步）、
        // v18 = 通知/震动/声音整族封装（host-notify 域收编 host-events.notify），
        // 叠加 v17 host-terminal/terminal-hooks 整面退役、v16 认证/配对编排下沉、
        // v15 终端订阅协议客户端迁插件、v14 host-websocket 客户端域、
        // v13 接收编排下沉、v12 发送编排下沉、
        // v11 host-mdns v2、v10 总线二进制载荷与 v9 host-peer 传输控制二原语
        assert_eq!(ABI_VERSION, 19);
    }
}
