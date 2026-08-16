# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# ==================== BedCode Custom Rules ====================

# Preserve debugging info for crash reports
-keepattributes SourceFile,LineNumberTable
-keepattributes *Annotation*

# Keep all generated Tauri classes
-keep class com.bedcode.mobile.generated.** { *; }

# Keep JavaScript interface methods
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}

# OkHttp (used by Tauri networking)
-dontwarn okhttp3.**
-keep class okhttp3.** { *; }
-keep interface okhttp3.** { *; }

# Preserve Kotlin metadata for reflection
-keep class kotlin.Metadata { *; }
-keepclassmembers class **$WhenMappings {
    <fields>;
}

# Rust JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# ==================== Tauri Kotlin plugins (R8 minification) ====================
# 自定义 Kotlin 插件（PluginAssetExtractor / ForegroundServicePlugin）由 Rust 端
# register_android_plugin 经 JNI 反射实例化，@Command 方法也由 PluginHandle
# 反射分发（indexMethods 读取 @Command 注解）。R8 无法看到这些反射引用，
# release minification 会把 @Command 方法整体剥离，导致
# "No command xxx found for plugin ..."。
# 已为各插件类标注 @TauriPlugin 以命中 Tauri 消费端 keep 规则；此处再按基类
# 兜底保留所有 Plugin 子类的构造器与成员，新增插件无需额外配置。
-keep class * extends app.tauri.plugin.Plugin {
    public <init>(...);
    *;
}

# @InvokeArg 参数类经 Jackson 反射反序列化（parseArgs），同样兜底保留。
# （Tauri 消费端已有 -keep @InvokeArg 规则，此处防御注解丢失的情况）
-keep class app.tauri.annotation.** { *; }
-keepclassmembers class * {
    @app.tauri.annotation.Command <methods>;
}