#!/bin/bash
# BedCode: adb client fd0 修复 shim
#
# 背景（详见 .scratch/adb-fd0-bug/bug-report.md）：
#   platform-tools 37.0.1 的 adb client 在被以「fd 0 关闭或阻塞在管道」的方式 spawn 时，
#   connect() 会落到坏 fd 上导致 smart-socket 握手失败（"ADB server didn't ACK"），
#   于是客户端误判 daemon 未运行，fork 一个新 fork-server，bind(5037) EADDRINUSE 后
#   SIGABRT（/tmp/adb.1000.log 每 2 秒一条崩溃），客户端以退出码 1 结束。
#   tauri CLI 的 `adb shell pidof <pkg>` 轮询循环正是用 duct（stdout/stderr 捕获时
#   stdin 为管道/关闭）spawn adb —— 循环永不成功，logcat 转发永不启动，
#   移动端 dev 日志永远到不了开发者控制台。
#
# 修复：stdin 非 TTY 时重开到 /dev/null，让 adb client 的 connect() 永远拿到干净 fd；
#       TTY 场景（终端里交互式 adb shell）原样透传。
# 由 dev-run.js 预检幂等安装（adb → adb.real + 本 shim），platform-tools 更新后下次
# dev 会话自愈。
if [ ! -t 0 ]; then exec 0</dev/null; fi
exec "$(dirname "$0")/adb.real" "$@"
