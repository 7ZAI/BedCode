#!/usr/bin/env bash
# 下载 onnxruntime-android .so 到 gen/android jniLibs（运行时 dlopen 用，规格 §4.6）
#
# ort 的 download-binaries 不支持 Android 目标，必须手动放官方 AAR 的 .so；
# 幂等：目标文件已存在且大小一致时跳过。gen/android 重建后需重新执行
# （AGENTS.md「Android」节恢复清单）。
#
# 用法: sh scripts/fetch-ort-android.sh [VERSION]
set -euo pipefail

ORT_VERSION="${1:-1.20.0}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
JNI="$ROOT/src-tauri/gen/android/app/src/main/jniLibs"
AAR_URL="https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/${ORT_VERSION}/onnxruntime-android-${ORT_VERSION}.aar"

ABIS=(arm64-v8a x86_64)

for abi in "${ABIS[@]}"; do
  mkdir -p "$JNI/$abi"
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "== downloading onnxruntime-android ${ORT_VERSION} (AAR)"
curl -fsSL "$AAR_URL" -o "$TMP/ort.aar"
unzip -oq "$TMP/ort.aar" -d "$TMP/aar"

for abi in "${ABIS[@]}"; do
  src="$TMP/aar/jni/$abi/libonnxruntime.so"
  dst="$JNI/$abi/libonnxruntime.so"
  if [ -f "$src" ]; then
    if [ -f "$dst" ] && cmp -s "$src" "$dst"; then
      echo "== $abi: already up to date"
    else
      cp "$src" "$dst"
      echo "== $abi: installed ($(du -h "$dst" | cut -f1))"
    fi
  else
    echo "!! $abi: libonnxruntime.so not in AAR" >&2
    exit 1
  fi
done

echo "== done: jniLibs/{${ABIS[*]}}/libonnxruntime.so"
