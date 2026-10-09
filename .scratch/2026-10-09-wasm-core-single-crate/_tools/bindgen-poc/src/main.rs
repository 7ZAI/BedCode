//! 票 01 P1：验证「能力域自持 WIT 分片（`world cap-pty`）」bindgen 出的 provider 侧
//! `Host` trait 与「整份端 WIT（`world plugin`）」版本**对等**。
//!
//! 判据设计（编译期即证，不需要运行 guest）：
//! 1. 下列 `impl Host for ProbeHost` 的**方法签名逐项复制自生产代码**
//!    `packages/bedcode-pty-engine/src/plugin_binding.rs:185`——那份 impl 正是「整份
//!    world 绑定」的实现，能通过编译 ⇒ 该签名 == 整份版的 trait 签名。
//!    本文件对**分片版** trait 写同样签名，能编译 ⇒ 两版签名一致（P1-a 得证）。
//! 2. `host_pty::add_to_linker` 可调用且首次注册成功；同一 interface 注册两次须报错
//!    （`defined twice`）⇒ 分片装配面成立（P1-b 得证）。

mod slice {
    wasmtime::component::bindgen!({
        path: "wit/pty.wit",
        world: "cap-pty",
    });
}

use wasmtime::component::HasSelf;

use slice::bedcode::plugin::host_pty::{self as host_pty, Host};

/// 探针宿主：只验证签名形状，方法体永不执行
struct ProbeHost;

impl Host for ProbeHost {
    fn spawn(&mut self, _config_json: String) -> Result<String, String> {
        unreachable!("POC 探针：不执行")
    }

    fn write(&mut self, _pty_id: String, _data: Vec<u8>) -> Result<(), String> {
        unreachable!("POC 探针：不执行")
    }

    fn resize(&mut self, _pty_id: String, _cols: u16, _rows: u16) -> Result<(), String> {
        unreachable!("POC 探针：不执行")
    }

    fn kill(&mut self, _pty_id: String) -> Result<(), String> {
        unreachable!("POC 探针：不执行")
    }

    fn ring_fetch(
        &mut self,
        _pty_id: String,
        _from_offset: u64,
        _max_bytes: u32,
    ) -> Result<Option<host_pty::RingFetchResult>, String> {
        unreachable!("POC 探针：不执行")
    }

    fn is_running(&mut self, _pty_id: String) -> Result<bool, String> {
        unreachable!("POC 探针：不执行")
    }
}

fn main() {
    let engine = wasmtime::Engine::default();
    let mut linker = wasmtime::component::Linker::<ProbeHost>::new(&engine);

    // 与生产同形的显式泛型（pty-engine/src/plugin_binding.rs:106 同款）
    // P1-b ①：分片版 add_to_linker 可调用且注册成功
    host_pty::add_to_linker::<ProbeHost, HasSelf<ProbeHost>>(&mut linker, |s| s).expect("首次注册应成功");

    // P1-b ②：同一 interface 注册两次须失败（defined twice）——越权装配会被抓住
    let dup = host_pty::add_to_linker::<ProbeHost, HasSelf<ProbeHost>>(&mut linker, |s| s);

    println!("P1-a 分片版 Host trait 签名 == 整份 world 版（编译通过即证）");
    println!("P1-b add_to_linker 首次=Ok；重复注册 is_err = {}", dup.is_err());
    if let Err(e) = &dup {
        println!("       重复注册错误：{e}");
    }
}
