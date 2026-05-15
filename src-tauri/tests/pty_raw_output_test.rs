//! PTY Raw Output Test
//!
//! 测试：启动 PTY 并获取原始输出数据（在 Base64 编码之前）

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{BufReader, Read};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    tracing::info!("Starting PTY raw output test...");

    // 1. 创建 PTY pair
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    tracing::info!("PTY pair created");

    // 2. 获取写入器和读取器
    let mut writer = pair.master.take_writer()?;
    let reader = pair.master.try_clone_reader()?;

    // 3. 启动子进程（使用简单的 echo 命令来测试）
    let mut cmd = CommandBuilder::new("powershell.exe");
    cmd.arg("-NoLogo");
    cmd.arg("-NoExit");
    cmd.arg("-Command");
    cmd.arg("echo 'Hello from PTY'; Write-Host 'Test output'; dir");

    let child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id();
    tracing::info!("Process spawned with PID: {:?}", pid);

    // 4. 启动读取线程来捕获原始输出
    let reader_handle = thread::spawn(move || {
        let mut buf_reader = BufReader::new(reader);
        let mut buffer = [0u8; 4096];

        // 读取前几次输出
        for i in 0..5 {
            match buf_reader.read(&mut buffer) {
                Ok(0) => {
                    tracing::info!("EOF received after {} reads", i);
                    break;
                }
                Ok(n) => {
                    // 打印原始字节数据
                    let raw_data = &buffer[..n];
                    tracing::info!("=== Raw Output #{} ({} bytes) ===", i + 1, n);

                    // 1. 打印原始字节（十六进制）
                    print!("Hex: ");
                    for byte in raw_data.iter().take(50) {
                        print!("{:02x} ", byte);
                    }
                    if n > 50 {
                        print!("... ({} more bytes)", n - 50);
                    }
                    println!();

                    // 2. 打印原始字节（十进制）
                    print!("Dec: ");
                    for byte in raw_data.iter().take(50) {
                        print!("{} ", byte);
                    }
                    if n > 50 {
                        print!("... ({} more bytes)", n - 50);
                    }
                    println!();

                    // 3. 尝试作为 UTF-8 字符串打印
                    match std::str::from_utf8(raw_data) {
                        Ok(s) => {
                            println!("String (UTF-8): {}", s);
                        }
                        Err(e) => {
                            println!("String (UTF-8 error): {} - valid up to: {:?}", e, std::str::from_utf8(&raw_data[..e.valid_up_to()]));
                        }
                    }

                    // 4. 打印 Base64 编码（当前实现使用的编码）
                    let b64 = base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        raw_data
                    );
                    println!("Base64: {}", b64);
                    println!();
                }
                Err(e) => {
                    tracing::error!("Read error: {}", e);
                    break;
                }
            }

            // 短暂休眠，避免过早读完
            thread::sleep(Duration::from_millis(100));
        }

        tracing::info!("Reader thread finished");
    });

    // 5. 等待一段时间让进程产生输出
    thread::sleep(Duration::from_secs(2));

    // 6. 写入一些输入触发更多输出
    writer.write_all(b"exit\n")?;
    writer.flush()?;

    // 7. 等待读取线程完成
    let _ = reader_handle.join();

    tracing::info!("Test completed");
    Ok(())
}