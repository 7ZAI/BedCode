# 移动端存储访问采用 SAF + MediaStore（不依赖 MANAGE_EXTERNAL_STORAGE）

个人开发应用无法通过 Google Play 政策获取 MANAGE_EXTERNAL_STORAGE（应用类别不符），且国产 ROM（MIUI/HyperOS）该权限入口不稳定、用户普遍拒绝授予；决定移动端文件传输的存储访问完全基于 SAF（Storage Access Framework）+ MediaStore：文件获取/共享目录/自定义落点走 SAF URI（`content://tree/...`）持久化授权，公共下载落点走 MediaStore.Downloads（API 29+ 零权限、系统文件管理器可见），app 私有目录仅作免授权特殊条目与写入失败回退。All Files 引导（`AllFilesAccessPlugin` + `needs_all_files_access` notice）仅保留为可选体验增强，不作为任何功能依赖。

## Considered Options

- **MANAGE_EXTERNAL_STORAGE 直读真实路径**：被拒——Play 政策拿不到（声明表审核要求核心功能为本地文件管理）；国产 ROM 入口不稳定；用户拒绝率高
- **仅 app 私有目录**：被拒——用户无法感知下载文件（藏在 `Android/data/`），共享范围受限
- **Shizuku（adb shell 提权）**：被拒——个人自用可行，但对外分发不现实（要求用户开启无线调试）
