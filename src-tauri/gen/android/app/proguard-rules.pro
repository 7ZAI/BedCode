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
-keep class com.bedcode.app.generated.** { *; }

# Keep custom classes (ForegroundService, TaskNotification, etc.)
-keep class com.bedcode.app.ForegroundService { *; }
-keep class com.bedcode.app.ForegroundServicePlugin { *; }
-keep class com.bedcode.app.TaskNotificationPlugin { *; }
-keep class com.bedcode.app.TaskNotificationManager { *; }
-keep class com.bedcode.app.TaskNotificationArgs { *; }
-keep class com.bedcode.app.CancelTaskNotificationArgs { *; }

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