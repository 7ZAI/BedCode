//! 认证中心裁决后的**连接身份**（ADR 0033）——**server-lib-split 迁移**：
//! 真源已随 `bedcode-server-base` 拆出（拆分后为 `bedcode-server-base` crate 的
//! `identity` 模块）；本文件保留模块路径与公开名字，宿主其余代码经
//! `crate::utils::auth::identity::*` 引用不受影响。类型语义见真源模块头注释。

pub use bedcode_server_base::identity::AuthenticatedIdentity;
