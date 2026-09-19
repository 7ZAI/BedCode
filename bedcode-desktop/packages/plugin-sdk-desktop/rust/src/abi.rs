//! WASM ABI 版本契约
//!
//! 迁移阶段 C 后插件产物统一为 Component Model 组件（WIT 契约，
//! 见 `wit/bedcode.wit`），名称常量与签名表（core 形态的 (ptr,len) ABI）
//! 已全部删除，此处仅保留组件通过 WIT `abi` 接口声明版本与形态的常量。
//!
//! # 版本演进
//!
//! 版本号语义不变（与历史 core ABI 共用同一序列）：
//! - v1: 初始版本（27 个 host functions + 11 个插件导出）
//! - v2: 新增 4 个参数绑定 SQL host functions（*_params），消灭插件侧手写转义
//! - v3: 新增提交输入行观察扩展点（host function `SESSION_INPUT_REGISTER`
//!   + 可选导出 `ON_INPUT_SUBMITTED`），见 ADR 0001
//! - v4: 新增插件状态上报扩展点（host function `MARK_PLUGIN_ERROR`），
//!   插件自检失败（如 hooks 配置失败）时上报宿主标记错误并通知前端
//! - v5: 新增通用文件服务能力（host functions `FILESRV_*` / `TRANSFER_*`
//!   + 可选导出 `ON_UPLOAD_REQUEST` 上传策略钩子），见内网文件传输插件规格
//! - v6: 新增会话创建与宿主定时器（host functions `SESSION_CREATE` /
//!   `TIMER_REGISTER`），支撑插件定时自动任务，见 ADR 0003
//! - v7: 新增会话关闭（host function `SESSION_CLOSE`），支撑插件在
//!   定时自动任务执行完后关闭其创建的会话
//! - v8: 生命周期契约补全（WIT `on-startup`/`on-shutdown` 携带
//!   `result<_, string>`），启动初始化/清理的失败可如实上抛宿主，
//!   宿主据此进入 Degraded 终态而非静默标记 Activated
//! - v9: host-peer 原语化收缩第一阶段（ADR 0022 v2，issue 13）：新增
//!   `dial-peer-endpoint` / `close` / `set-shared-roots` 三原语与旧函数并存；
//!   新增 `host-mdns`（browse-only）与 `host-platform` 接口。纯增量变更，
//!   v8 插件二进制不受影响
//! - v10: host-peer WIT 收缩 ADR 0022 v3 终态（commit 56ee094cb）：host-peer
//!   桌面终态 13 函数定稿（dial-peer 转正、send-files 返回传输句柄、删除
//!   旧式寻址函数集）。破坏性收缩，旧插件二进制须重编译
//! - v11: host-peer 传输控制三原语（`pause-transfer` / `resume-transfer` /
//!   `resume-all-transfers`），支撑暂停/恢复（issue 14）。纯增量变更，
//!   v10 插件二进制不受影响
//! - v12: 消息总线二进制载荷（host-bus.publish-binary / subscribe-binary）：
//!   零 JSON 编解码、可传非 UTF-8 与大载荷；新增可选导出 `events-binary`
//!   （宿主实例化后动态探测，旧插件不导出则只收 JSON，不受影响）。
//!   注：v12 为 dev（v11 host-peer 三原语）与本分支（总线二进制）合并后的
//!   版本——两边曾各自把 v11 用于不同语义，合并后宿主能力为两者超集，
//!   声明 v11 及以下的插件二进制仍可加载（后续统一重编译再对齐）
//! - v13: host-mdns v2（mDNS 基础能力服务契约）：新增
//!   `advertise` / `stop-advertise` / `is-advertising` 三原语 + 浏览事件
//!   定向投递 `mdns:found.<owner>` / `mdns:lost.<owner>`（payload 增
//!   serviceType / browserId 字段）。纯增量变更，v12 插件二进制不受影响
//! - v14: host-websocket 基础能力服务（WS 传输原语）：新增
//!   `host-websocket`（客户端域 connect/send-text/send-binary/close/
//!   is-connected + 服务端域 register-endpoint/收发/广播/踢出/注销/清单）
//!   与可选导出 `events-ws`（宿主动态探测，未导出 → 消息帧丢弃 + 首次
//!   warn + 计数）。纯增量变更，v13 插件二进制不受影响
//! - v15: 密钥托管原语（host-auth / secret-store）：`get/set/delete/keys` 四函数，
//!   属主隔离 + 权限门（PERMISSION_AUTH）+ 持久化（plugin_secrets 表）+
//!   明文不落日志。认证中心语义下沉的宿主侧前置（host-pty 线让位 v16，
//!   见 .scratch/2026-09-19-pty-base-service/spec.md D7）。新接口走 bump，
//!   v14 及以下的插件二进制仍可加载（`version > 当前 → 拒绝` 语义）
//! - v16: 插件私有伪终端原语（host-pty）：`spawn/write/resize/kill/ring-fetch/
//!   is-running` 六函数 + `pty:exit.<owner>` 事件（spec D7）。输出面为
//!   「单生产者环形缓冲 + 插件拉取游标」纯拉取模型（无 push 回调，D3）；
//!   属主隔离 + 权限两域（pty:spawn / pty:io）。纯增量新接口，v15 及以下
//!   插件二进制不受影响
pub const ABI_VERSION: u32 = 16;

/// 组件形态标识：`abi.form() == FORM_COMPONENT`（WIT `abi` 接口的 form() 声明）
///
/// 组件通过 WIT `abi` 接口的 `form()` 声明形态，语义与 ABI_VERSION 解耦：
/// 不 bump ABI 大版本，仅区分加载路径（core module vs component，
/// 后者为迁移后唯一形态；FORM_CORE=0 已在阶段 C 删除）
pub const FORM_COMPONENT: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abi_version_is_v16() {
        // 版本号序列与历史 core ABI 共用：v16 = 插件私有伪终端原语（host-pty），
        // 叠加 v15 密钥托管（host-auth / secret-store）、v14 host-websocket、
        // v13 host-mdns v2、v12 总线二进制载荷与 v11 host-peer 传输控制三原语
        assert_eq!(ABI_VERSION, 16);
    }

    #[test]
    fn test_form_component_constant() {
        // 迁移阶段 C 后唯一形态是组件；FORM_CORE=0 已删除，
        // 锁定 1 防未来误引入 0 值导致宿主加载路径回退
        assert_eq!(FORM_COMPONENT, 1);
        assert_ne!(FORM_COMPONENT, 0);
    }
}
