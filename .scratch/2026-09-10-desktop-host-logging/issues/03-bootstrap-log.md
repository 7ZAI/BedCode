# 03: 启动早期 bootstrap 日志

**What to build:** build_logging 之前（config 加载、默认配置复制、dev reset 等启动早期路径，`lib.rs:150-168`）的日志不再只走 `eprintln!`——新增进程级 bootstrap 通道：`bootstrap.log`（应用日志目录，rolling::never + non_blocking），`init_logging` 完成后由 runtime 文件接管。release 构建启动早期证据不丢（config 加载失败等）。

**Blocked by:** None

**Status:** done

- [x] bootstrap writer：日志目录（与 runtime 同目录，创建幂等）、non_blocking worker、guard 进程级存活；每次写重建文件句柄（dev reset 删除后自动重建）
- [x] 启动早期路径改走 bootstrap 日志：config.properties 复制失败、config 加载失败、dev reset——eprintln 由 bootstrap 层统一双写，release 不丢
- [x] `init_logging` 接管：bootstrap.log 停止增长（bootstrap_log 仅启动早期路径调用），runtime.*.log 继续；容量裁剪（.log 后缀）天然覆盖 bootstrap.log
- [x] panic hook 与日志系统自身未就绪路径行为不变（panic.log 独立）
- [x] 测试：临时目录单测断言 bootstrap.log 创建且内容非空、删除后重建；错误串带操作上下文（无裸字符串）