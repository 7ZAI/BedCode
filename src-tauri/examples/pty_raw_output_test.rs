//! PTY Raw Output Test - Standalone Binary
//!
//! 测试：启动 PTY 并获取原始输出数据（在 Base64 编码之前）
//!
//! 运行: cargo run --bin pty_raw_output_test

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{BufReader, Read};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PTY Raw Output Test ===\n");

    // 1. 创建 PTY pair
    println!("1. Creating PTY pair...");
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    println!("   PTY pair created successfully\n");

    // 2. 获取写入器和读取器
    println!("2. Getting writer and reader...");
    let mut writer = pair.master.take_writer()?;
    let reader = pair.master.try_clone_reader()?;
    println!("   Writer and reader obtained\n");

    // 3. 启动子进程 - 使用简单的 echo 测试
    println!("3. Spawning process...");
    let mut cmd = CommandBuilder::new("powershell.exe");
    cmd.arg("-NoLogo");
    cmd.arg("-NoExit");
    cmd.arg("-Command");
    cmd.arg("Write-Host 'Test started'; echo 'Hello from PTY'");

    let child = pair.slave.spawn_command(cmd)?;
    let pid = child.process_id();
    println!("   Process spawned with PID: {:?}\n", pid);

    // 4. 读取原始输出
    println!("4. Reading raw PTY output...\n");
    let mut buf_reader = BufReader::new(reader);
    let mut buffer = [0u8; 4096];
    let mut read_count = 0;

    // 读取前几次输出
    for i in 0..5 {
        match buf_reader.read(&mut buffer) {
            Ok(0) => {
                println!("   EOF received after {} reads\n", i);
                break;
            }
            Ok(n) => {
                read_count += 1;
                let raw_data = &buffer[..n];
                println!("=== Raw Output #{} ({} bytes) ===", i + 1, n);

                // 打印原始字节（十六进制）
                print!("   Hex: ");
                for byte in raw_data.iter().take(30) {
                    print!("{:02x} ", byte);
                }
                if n > 30 {
                    print!("... ({} more)", n - 30);
                }
                println!();

                // 打印原始字节（十进制）
                print!("   Dec: ");
                for byte in raw_data.iter().take(30) {
                    print!("{} ", byte);
                }
                if n > 30 {
                    print!("... ({} more)", n - 30);
                }
                println!();

                // 尝试作为 UTF-8 字符串打印
                match std::str::from_utf8(raw_data) {
                    Ok(s) => {
                        println!("   String (UTF-8): {:?}", s.trim());
                    }
                    Err(e) => {
                        let valid = std::str::from_utf8(&raw_data[..e.valid_up_to()]);
                        println!("   String (UTF-8 error): {}, valid: {:?}", e, valid);
                    }
                }

                // 打印 Base64 编码
                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    raw_data
                );
                println!("   Base64: {}", b64);
                println!();
            }
            Err(e) => {
                println!("   Read error: {}", e);
                break;
            }
        }

        thread::sleep(Duration::from_millis(200));
    }

    // 5. 清理
    println!("5. Cleanup...");
    let _ = writer.write_all(b"exit\n");
    let _ = writer.flush();

    println!("\n=== Test Complete ===");
    println!("Total reads: {}", read_count);

    Ok(())
}