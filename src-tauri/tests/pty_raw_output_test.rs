//! PTY Raw Output Test
//!
//! 测试：启动 PTY 并获取原始输出数据（在 Base64 编码之前）

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{BufReader, Read};
use std::thread;
use std::time::Duration;

#[test]
fn test_pty_raw_output() {
    // 初始化日志
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    tracing::info!("Starting PTY raw output test...");

    // 1. 创建 PTY pair
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }).expect("Failed to open PTY");

    tracing::info!("PTY pair created");

    // 2. 获取写入器和读取器
    let mut writer = pair.master.take_writer().expect("Failed to get writer");
    let reader = pair.master.try_clone_reader().expect("Failed to get reader");

    // 3. 启动子进程
    let mut cmd = CommandBuilder::new("powershell.exe");
    cmd.arg("-NoLogo");
    cmd.arg("-NoExit");
    cmd.arg("-Command");
    cmd.arg("echo 'Hello from PTY'; Write-Host 'Test output'");

    let child = pair.slave.spawn_command(cmd).expect("Failed to spawn command");
    let pid = child.process_id();
    tracing::info!("Process spawned with PID: {:?}", pid);

    // 4. 读取原始输出
    let mut buf_reader = BufReader::new(reader);
    let mut buffer = [0u8; 4096];

    // 读取前几次输出
    let mut total_reads = 0;
    for i in 0..5 {
        match buf_reader.read(&mut buffer) {
            Ok(0) => {
                tracing::info!("EOF received after {} reads", i);
                break;
            }
            Ok(n) => {
                total_reads += 1;
                let raw_data = &buffer[..n];
                tracing::info!("=== Raw Output #{} ({} bytes) ===", i + 1, n);

                // 打印原始字节（十六进制）
                let hex: String = raw_data.iter().take(30)
                    .map(|b| format!("{:02x} ", b))
                    .collect();
                tracing::info!("Hex: {}", hex);

                // 尝试作为 UTF-8 字符串打印
                match std::str::from_utf8(raw_data) {
                    Ok(s) => {
                        tracing::info!("String (UTF-8): {}", s.trim());
                    }
                    Err(e) => {
                        let valid = std::str::from_utf8(&raw_data[..e.valid_up_to()]);
                        tracing::info!("String (UTF-8 error): {}, valid: {:?}", e, valid);
                    }
                }

                // 打印 Base64 编码（当前实现使用的编码）
                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    raw_data
                );
                tracing::info!("Base64: {}", b64);
            }
            Err(e) => {
                tracing::error!("Read error: {}", e);
                break;
            }
        }

        thread::sleep(Duration::from_millis(200));
    }

    // 5. 清理
    let _ = writer.write_all(b"exit\n");
    let _ = writer.flush();

    // 验证至少读取到一些数据
    assert!(total_reads > 0, "Should have read at least some output");

    tracing::info!("Test completed, total reads: {}", total_reads);
}