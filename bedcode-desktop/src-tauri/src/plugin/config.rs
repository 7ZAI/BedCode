//! 配置模块（core-config）
//!
//! wasmtime Engine 构建参数与 Store 资源上限的单一配置面
//! （分层：编译期默认 < 配置文件 < 运行时覆盖）。
//!
//! 骨架——配置结构与分层加载在票据 02 落地；
//! 当前运行参数仍以 `manager::wasm_runtime` 顶部常量为准。
