status: claimed
# 01: 日志构建 seam 化 + 非阻塞异步落盘

**What to build:** 把日志系统构建从应用启动引导中抽出为可独立测试的纯函数 seam（输入日志目录 + LogConfig，返回订阅器与句柄集），应用启动只做薄封装；三个文件层（runtime/error/frontend）全部改为非阻塞异步写盘（有界缓冲 + worker 线程）。用户可感知的行为不变：按天命名、dev 重置当天日志、`Logging initialized.` 首行照旧，但高频输出不再被同步文件 I/O 阻塞。

**Blocked by:** None（可立即开工）

**Status:** resolved

- [x] 日志构建 seam 不依赖应用句柄，接受日志目录与 LogConfig 即可构造订阅器；现有启动逻辑降为薄封装，行为不变
- [x] 应用启动后 runtime/*.log、error/*.log（dev 另有 frontend/*.log）照常按天生成，`Logging initialized.` 为文件首行，内容格式与改造前一致
- [x] 错误层仅 ERROR、runtime 层按配置级别、frontend 层仅 dev 且 target=`frontend` 的过滤语义在 seam 上通过单元测试（临时目录真实落盘断言）
- [x] 三层均走非阻塞写盘：worker guard 进程级存活，退出前 flush；队列满丢弃可计数，不静默
- [x] `cargo test` 全量绿

## Answer

已完成（2026-09-09）。

**实现要点**：
- `system/logging.rs` 新增 `build_logging(log_dir, log_config, dev) -> (LoggingSetup, Box<dyn Subscriber>)` 主 seam：三个文件层（error/runtime/frontend）+ 控制台层全部非阻塞写盘（`NonBlockingBuilder` 有界缓冲 20k 行、满则丢 + `error_counter()` 计数），`LoggingSetup` 携 worker guard / reload 句柄 / 丢弃计数 / 日志目录，进程级 `OnceLock` 全局存活（02/03 从此取）
- `lib.rs` init_logging 降为薄封装：目录准备 + dev 重置当天日志 + 调 build_logging + install_subscriber + store_setup
- 组装模式说明：fmt::Layer 的 S 泛型必须在 `with` 嵌套链中自由推断（悬挂），不能封装帮助函数，也不能 Box<dyn Layer>（嵌套后 S 变 Layered 无法满足约束）；reload filter 必须挂在链首（S 解析为 Registry，句柄类型才能存入 LoggingSetup）；frontend/console 层永远存在，非 dev / 关闭时用 `DiscardMakeWriter` 占位 + filter off，避免类型分支
- **踩坑**：编辑时曾把 `split_plugin_tag` 的 doc comment 与其函数体隔开（doc 悬空挂到 LoggingSetup）——已被 clippy empty_line_after_doc_comments 暴露并修复，doc 移回函数前

**验证**：`cargo test --lib` 575 全绿（新增 3 个 seam 验收测试：release 语义级别过滤 + 无 frontend 文件、dev 强制 debug + frontend 文件、丢弃计数接口基准 0）；logging.rs clippy 0 提示。