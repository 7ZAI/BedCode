//! 宿主侧插件能力面：能力域端口 adapter 与宿主侧能力实现
//!
//! **为什么存在这个目录**（wasm-core 纯净性收口票 02，`.scratch/2026-10-09-wasm-core-single-crate/`）：
//! 内核只留「应用无关的通用引擎」；能力域实现搬进 `packages/` 的能力 crate 后，
//! 剩下**宿主侧那一半**（端口 adapter / 强制引用行 / 白名单条目 / 生命周期接线）
//! 一律落在本目录——每个域一个文件，几样东西**同处**，避免「注册在 A、装配在 B」
//! 的静默漂移。
//!
//! 接线口径（host-kit 装配自报面，见 `bedcode_host_kit::assembly` 模块文档）：
//!
//! - [`bedcode_host_kit::expect_host_module`]：把本域写进能力模块白名单（与强制
//!   引用行同处；漏一处 ⇒ 装载期 unlisted / missing 方向显性点名）；
//! - `use <crate> as _;`：把该 crate 的 `inventory` 自报静态链进最终二进制；
//! - [`bedcode_host_kit::submit_domain_ports_installer`]：声明本域的端口装配器，
//!   内核装配链（`install_capability_domain_ports`）在任何插件实例化**之前**遍历调用。
//!
//! 迁移批次（票 02）：批次 02 = pty（本目录首个域）；批次 03 = http / ws / peer /
//! mdns；批次 04 = auth / crypto（路径 B 首两域）；批次 05 = task / process / app /
//! timer / connection / api_call（路径 B；api_call 是薄转发，编排留内核）；
//! 批次 06 = 切片域的桌面扩展（fs / platform / events / abi-form）：v36 交集接口
//! 切片——四个交集接口拆出桌面独有函数为新 interface（host-fs-desktop /
//! host-platform-desktop / host-events-desktop / abi-form），桌面扩展的 WIT impl
//! 落宿主（events.rs / fs.rs / platform.rs，路径 B；abi-form 是 guest 导出，SDK
//! 宏 + 宿主 verify_abi 消费，宿主无实现）；实现本体除 fs 三函数（内核
//! `host_api/fs.rs` 提 pub 双消费者机制）外随域走宿主。

pub mod api_call;
pub mod app;
pub mod auth;
pub mod bindings;
pub mod connection;
pub mod crypto;
pub mod events;
pub mod fs;
pub mod http;
pub mod mdns;
pub mod peer;
pub mod platform;
pub mod process;
pub mod pty;
pub mod task;
pub mod timer;
pub mod ws;
