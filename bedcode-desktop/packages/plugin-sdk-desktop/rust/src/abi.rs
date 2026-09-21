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
//! - v17: 认证策略导出（auth-policy / `verify-device-token`，desktop 独有，双端
//!   偏离同 host-auth）：认证中心能力，宿主 server 中间件验签后取策略（票 12 C3）。
//!   可选导出（宿主动态探测），非认证中心插件以 SDK 默认实现导出（拒绝）。
//!   纯增量新接口，v16 及以下插件二进制不受影响
//! - v18: host-auth 认证记录面（`trusted-devices-list` / `trusted-device-revoke` /
//!   `connection-history-list` / `auth-setting-set`，desktop 独有，双端偏离同
//!   host-auth）：会话语义下沉批次（终端会话中心插件票 05，spec
//!   `.scratch/2026-09-19-terminal-session-plugin` D4）——既有 interface 的
//!   函数级追加，返回内核原始记录、排序与解读归插件。纯增量，v17 及以下插件
//!   二进制不受影响
//! - v19: host-session 配置面（`config-upsert` / `config-get` / `config-delete`，
//!   desktop 独有，双端偏离同 host-session）：会话语义下沉批次之二（同一 spec
//!   票 07，权限 `session:config`）——配置 CRUD 交给插件，真源与校验规则仍在
//!   内核（票 08 才迁真源）。既有 interface 的函数级追加，纯增量，v18 及以下
//!   插件二进制不受影响。票 09 / 10 同 v19 追加会话创建与动作面（函数级追加
//!   不 bump）：`create-with-spec`（插件算好 launch spec，宿主只做 shell 包装 /
//!   WSL 转换 / 尺寸缺省 / ID 预生成——「映射决策归插件、执行留内核」的关键切口，
//!   权限 `session:write`）+ `restart` / `remove` / `rename` / `resize`（`resize`
//!   带请求端标识：裁决规则在插件、谁是当前渲染端的事实登记在内核）。
//!   票 11 同 v19 再加两函数（函数级追加不 bump）：
//!   `annotate`（会话注解槽写入，权限 `session:write`，spec D5 内核去业务化的
//!   expand 期并行写入面）+ `connections-list`（连接注册表原始记录清单，无排序
//!   无解读，权限 `session:read`——设备派生视图真源，替代硬编码 0 的会话数）。
//!   票 13 再于 `host-platform` 追加一函数（同为函数级追加不 bump）：
//!   `wsl-distros`（WSL 发行版名枚举，宿主无 WSL 时显性报错而非空数组）——
//!   会话配置表单的执行环境分支需要平台事实，无业务语义（ADR 0022 裁剪线）
//!
//! - v20: host-task 宿主并发任务域（WASM 插件调度 OS 线程池真并行执行单元操作
//!   计划，desktop 独有双端偏离）
//! - v21: **host-session 收敛退役**（host-business-decarriage 收尾，desktop 独有，
//!   双端偏离同 host-session）：删除 `create`（legacy 按 configId 创建）与
//!   `restart`（内核重启执行器）两函数——创建编排统一走 `create-with-spec`
//!   （重启 = 插件侧 `remove` + 同 id `create-with-spec`，spec 增可选 `sessionId`
//!   字段，JSON 内部追加不属于接口变化）。这是**接口函数删除**（非增量），宿主
//!   不再提供这两个 import；旧产物（≤v20）若仍 import 它们将在实例化期被拒，
//!   需按 v21 SDK 重新构建。
//!
//! - v22: `host-platform.reveal-in-dir`（desktop 独有，双端偏离同 host-platform）：
//!   在系统文件管理器中定位并选中文件/目录的平台原语（票 04）。原先该能力只挂在
//!   宿主命令 `plugin_reveal_in_dir` + `system:open` 权限 + 前端 `context.system`
//!   桥上（「能力已存在但没有原语」的遗留形态），现按 ADR 0022 裁剪线归入
//!   `host-platform`——与 `pick-*` 同口径（平台交互动作、不读数据）故**不叠加
//!   权限门**，`system:open` 权限随宿主命令面与前端 API 一并退役（五同步点全落）。
//!   既有 interface 的函数级追加，纯增量，v21 及以下插件二进制不受影响。
//!
//! 编号口径（AGENTS.md §7 教训）：本号以 `abi.rs` 与 WIT 版本表实读为准，
//! 规格正文的「17 → 19」是并发线（host-notification v18）尚未落地时的预判；
//! 本分支实测 v18 已被「host-auth 认证记录面」占用（票 05），故会话语义下沉的
//! 第二批次取 v19。
pub const ABI_VERSION: u32 = 22;

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
    fn test_abi_version_is_v22() {
        // 版本号序列与历史 core ABI 共用：v22 = host-platform.reveal-in-dir（平台定位
        // 原语，无权限门；`system:open` 随之退役），叠加 v21 host-session 收敛退役
        // （删 `create` / `restart`）、v20 host-task 宿主并发任务域、
        // v19 host-session 配置面与创建/动作面（会话语义下沉批次之二）、
        // v18 host-auth 认证记录面（同批次之一）、v17 认证策略导出（auth-policy）、
        // v16 插件私有伪终端原语（host-pty）、v15 密钥托管（host-auth / secret-store）、
        // v14 host-websocket、v13 host-mdns v2、v12 总线二进制载荷与 v11 host-peer 传输控制三原语
        assert_eq!(ABI_VERSION, 22);
    }

    #[test]
    fn test_form_component_constant() {
        // 迁移阶段 C 后唯一形态是组件；FORM_CORE=0 已删除，
        // 锁定 1 防未来误引入 0 值导致宿主加载路径回退
        assert_eq!(FORM_COMPONENT, 1);
        assert_ne!(FORM_COMPONENT, 0);
    }
}
