//! AI Chatbox Plugin (cdylib)
//!
//! AI 大模型对话与终端提示词优化
//! 独立编译为 cdylib 动态库，通过 C ABI + JSON 与宿主通信

mod host_api;
mod ai_client;
mod db;
mod commands;
mod terminal;

use host_api::HostContext;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// 宿主注入的上下文（activate 时设置，deactivate 时清除）
static HOST_CONTEXT: OnceLock<HostContext> = OnceLock::new();

/// 插件激活状态标记 — 停用后拒绝所有 API 调用
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// 检查插件是否处于激活状态，未激活时返回错误
fn ensure_active() -> anyhow::Result<()> {
    if !ACTIVE.load(Ordering::SeqCst) {
        anyhow::bail!("Plugin not activated");
    }
    Ok(())
}

/// 返回插件 manifest JSON
#[no_mangle]
pub extern "C" fn bedcode_plugin_manifest() -> *mut c_char {
    let json = serde_json::json!({
        "id": "com.bedcode.ai-chatbox",
        "name": "AI Chatbox",
        "version": "1.0.0",
        "description": "AI 大模型对话与终端提示词优化",
        "author": "BedCode",
        "main": "index.js",
        "sandbox": "inline",
        "pluginType": "rust-ts",
        "rustLibrary": "bedcode_plugin_ai_chatbox",
        "permissions": ["ui:sidebar", "ui:input", "storage", "terminal:input", "terminal:output", "session:read"],
        "contributes": {
            "commands": [
                { "id": "ai-chatbox.chat-stream", "title": "AI Chat Stream" },
                { "id": "ai-chatbox.chat-complete", "title": "AI Chat Complete" },
                { "id": "ai-chatbox.optimize-prompt", "title": "Optimize Prompt" },
                { "id": "ai-chatbox.list-conversations", "title": "List Conversations" },
                { "id": "ai-chatbox.get-messages", "title": "Get Messages" },
                { "id": "ai-chatbox.save-conversation", "title": "Save Conversation" },
                { "id": "ai-chatbox.save-message", "title": "Save Message" },
                { "id": "ai-chatbox.delete-conversation", "title": "Delete Conversation" }
            ],
            "views": [
                { "id": "ai-chatbox.sidebar", "type": "sidebar", "title": "AI 对话", "component": "ChatView" }
            ],
            "terminal": {
                "inputHandlers": ["on_terminal_input"],
                "outputParsers": []
            },
            "configuration": {
                "title": "AI Chatbox Settings",
                "properties": {
                    "apiProviders": { "type": "string", "title": "API Providers (JSON)", "description": "JSON array of API provider configs", "default": "[]" },
                    "activeProvider": { "type": "string", "title": "Active Provider ID", "default": "" },
                    "activeModel": { "type": "string", "title": "Active Model", "default": "" }
                }
            }
        }
    });
    CString::new(json.to_string()).unwrap().into_raw()
}

/// 激活插件，传入 HostContext
#[no_mangle]
pub extern "C" fn bedcode_plugin_activate(host_ctx: *const HostContext) -> c_int {
    if host_ctx.is_null() {
        // activate 阶段 HostContext 尚未设置，使用 tracing 作为 fallback
        tracing::error!("[plugin:com.bedcode.ai-chatbox] activate: null HostContext");
        return 1;
    }
    match HOST_CONTEXT.set(unsafe { std::ptr::read(host_ctx) }) {
        Ok(()) => {
            let host = HOST_CONTEXT.get().unwrap();
            // 初始化自定义数据库表
            if let Err(e) = db::init() {
                host.log_error(&format!("DB init failed: {}", e));
                return 2;
            }
            ACTIVE.store(true, Ordering::SeqCst);
            host.log_info("Plugin activated (cdylib)");
            0
        }
        Err(_) => {
            // 已激活（重复调用），标记为激活状态
            ACTIVE.store(true, Ordering::SeqCst);
            let host = HOST_CONTEXT.get().unwrap();
            host.log_warn("Plugin already activated, HostContext already set");
            0
        }
    }
}

/// 停用插件
#[no_mangle]
pub extern "C" fn bedcode_plugin_deactivate() -> c_int {
    ACTIVE.store(false, Ordering::SeqCst);
    if let Some(host) = HOST_CONTEXT.get() {
        host.log_info("Plugin deactivated (cdylib)");
    }
    0
}

/// 调用插件注册的自定义 command
#[no_mangle]
pub extern "C" fn bedcode_plugin_invoke_command(
    command_name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    // 激活状态检查
    if let Err(e) = ensure_active() {
        let error_json = serde_json::json!({ "error": e.to_string() });
        return CString::new(error_json.to_string()).unwrap().into_raw();
    }

    let name = if command_name.is_null() {
        if let Some(host) = HOST_CONTEXT.get() {
            host.log_error("invoke_command: null command_name");
        }
        return ptr::null_mut();
    } else {
        unsafe { CStr::from_ptr(command_name) }.to_str().unwrap_or("").to_string()
    };

    let args = if args_json.is_null() {
        "{}".to_string()
    } else {
        unsafe { CStr::from_ptr(args_json) }.to_str().unwrap_or("{}").to_string()
    };

    let result = match name.as_str() {
        "ai-chatbox.chat-stream" => commands::chat_stream(&args),
        "ai-chatbox.chat-complete" => commands::chat_complete(&args),
        "ai-chatbox.optimize-prompt" => commands::optimize_prompt(&args),
        "ai-chatbox.list-conversations" => commands::list_conversations(&args),
        "ai-chatbox.get-messages" => commands::get_messages(&args),
        "ai-chatbox.save-conversation" => commands::save_conversation(&args),
        "ai-chatbox.save-message" => commands::save_message(&args),
        "ai-chatbox.delete-conversation" => commands::delete_conversation(&args),
        _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
    };

    match result {
        Ok(val) => CString::new(val.to_string()).unwrap().into_raw(),
        Err(e) => {
            if let Some(host) = HOST_CONTEXT.get() {
                host.log_error(&format!("Command '{}' failed: {}", name, e));
            }
            let error_json = serde_json::json!({ "error": e.to_string() });
            CString::new(error_json.to_string()).unwrap().into_raw()
        }
    }
}

/// 终端输入处理器（MVP：不做修改，返回 null）
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_input(
    _input_json: *const c_char,
) -> *mut c_char {
    ptr::null_mut()
}

/// 终端输出处理器（MVP：不做修改，返回 null）
#[no_mangle]
pub extern "C" fn bedcode_plugin_on_terminal_output(
    _output_json: *const c_char,
) -> *mut c_char {
    ptr::null_mut()
}
