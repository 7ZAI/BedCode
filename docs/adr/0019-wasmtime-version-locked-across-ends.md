# wasmtime 版本两端锁死，升级必须同步

桌面端与移动端均依赖 `wasmtime = "47"`，版本必须两端一致、升级时一次性同步完成。约束来源：47 修复了 Android aarch64 平台 JIT/缓存稳定性问题（46 曾现 SIGILL 崩溃，见各端 `Cargo.toml` 注释）；两端 `.cwasm` AOT 产物按 Engine 版本序列化，跨端复用要求同版本。配套 binding 组合（wit-bindgen）随本次迁移定版（移动端目标 0.60.0、回退 0.41），升级时按组合一并锁定。