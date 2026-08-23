package com.bedcode.mobile

import android.app.Activity
import android.content.Context
import android.net.wifi.WifiManager
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Tauri 插件 - 多播锁持有者（对等网络 mDNS 发现的前置条件）
 *
 * 为什么需要：Android 为省电默认丢弃多播 UDP 包，应用必须持有
 * WifiManager.MulticastLock 才能收到 mDNS 响应。不持锁时广播发得出去、
 * 对端的响应进不来——表现为「我能广播但发现不了别人」的单侧可见。
 * CHANGE_WIFI_MULTICAST_STATE 权限已在 AndroidManifest.xml 声明。
 *
 * 设计要点（peer-network issue 03 决策 D6）：
 * - acquire/release 幂等：setReferenceCounted(false) 非引用计数模式，
 *   重复 acquire 不叠加计数，任意一次 release 即释放；Rust 侧再以 held
 *   状态查询兜底，双层防呆。
 * - 持锁生命周期 = 从对等网络发现守护启动到显式 release，刻意不绑
 *   Activity 生命周期：后台/锁屏期间（前台服务存活时）仍须可收包，
 *   这是 AC#3 后台可见性的前提。
 *
 * 由 Rust 端 android_plugins.rs 注册（Builder 名 "multicast-lock"）。
 * gen/android 重建恢复清单：本文件须恢复（参见 AGENTS.md「Android」节）。
 */
@TauriPlugin
class MulticastLockPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "BedCode-MulticastLock"
        /** 锁调试标签：dumpsys wifi 按此字符串定位持锁者 */
        private const val LOCK_TAG = "bedcode-peer-mdns"
    }

    /** 当前持有的锁实例；null = 未持锁 */
    private var lock: WifiManager.MulticastLock? = null

    /** 申请多播锁（幂等：已持锁直接返回 held=true） */
    @Command
    fun acquire(invoke: Invoke) {
        if (lock?.isHeld == true) {
            invoke.resolve(JSObject().apply { put("held", true) })
            return
        }
        try {
            val wifi =
                activity.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            val created = wifi.createMulticastLock(LOCK_TAG)
            created.setReferenceCounted(false)
            created.acquire()
            lock = created
            android.util.Log.i(TAG, "multicast lock acquired")
            invoke.resolve(JSObject().apply { put("held", true) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "multicast lock acquire failed: ${e.message}")
            invoke.reject("Failed to acquire multicast lock: ${e.message}")
        }
    }

    /** 释放多播锁（幂等：未持锁返回 held=false） */
    @Command
    fun release(invoke: Invoke) {
        val current = lock
        if (current == null || !current.isHeld) {
            // 状态自愈：isHeld=false 但实例残留时一并清空，避免下次误判
            lock = null
            invoke.resolve(JSObject().apply { put("held", false) })
            return
        }
        try {
            current.release()
            lock = null
            android.util.Log.i(TAG, "multicast lock released")
            invoke.resolve(JSObject().apply { put("held", false) })
        } catch (e: Exception) {
            android.util.Log.e(TAG, "multicast lock release failed: ${e.message}")
            invoke.reject("Failed to release multicast lock: ${e.message}")
        }
    }

    /** 查询当前持锁状态（排查用） */
    @Command
    fun isHeld(invoke: Invoke) {
        invoke.resolve(JSObject().apply { put("held", lock?.isHeld == true) })
    }
}
