---
name: BedCode Logo Design
description: Logo和App图标设计规格 - Claude风格背景 + h形床架 + 双箭头二进制输出
type: project
---

# BedCode Logo 设计规格

## 最终设计定稿

### 核心概念
- **床架形态**: h形拉宽（长竖线+短竖线+横线连接），表达"Bed"概念
- **持续输出**: 双箭头 `>>` 表示代码持续输出
- **Claude致敬**: 背景色 + C的ASCII二进制(01000011)

### 设计参数

| 参数 | 值 | 说明 |
|------|-----|------|
| Logo尺寸 | 120×75px | 基准尺寸 |
| 形状结构 | h形拉宽 | 长竖线(左) + 短竖线(右,底部对齐) + 横线连接 |
| 线条风格 | 一笔贯通 | stroke-width: 10，圆角端点/连接 |
| 背景颜色 | #c26a4d | Claude暖色调 |
| 线条颜色 | white | 白色 |
| 代码内容 | >>01000011 | 双箭头 + C的ASCII二进制 |
| 代码颜色 | #22c55e | 绿色 |
| 水平间隔比例 | 16.7% | >> 距左竖线 = 图标宽度×16.7% |
| 垂直间隔比例 | 13.3% | 代码距横线 = 图标高度×13.3% |
| 代码字体比例 | 12.5% | 字体大小 = 图标宽度×12.5% |

### SVG源码

```svg
<svg width="120" height="75" viewBox="0 0 120 75">
  <!-- h形床架线条 -->
  <path d="M 5 5 L 5 70 L 115 70 L 115 40 L 5 40"
        stroke="white" stroke-width="10"
        stroke-linecap="round" stroke-linejoin="round" fill="none"/>
  <!-- 代码文本（需按比例缩放） -->
  <text x="20" y="10" fill="#22c55e" font-family="monospace" font-weight="600">
    >>01000011
  </text>
</svg>
```

### 缩放公式

```
水平间隔 = 图标宽度 × 16.7%
垂直间隔 = 图标高度 × 13.3%
代码字体 = 图标宽度 × 12.5%
```

### 各尺寸计算示例

| 尺寸 | 水平间隔 | 垂直间隔 | 字体大小 |
|------|---------|---------|---------|
| 32×32 | 5.3px | 4.3px | 4px |
| 64×64 | 10.7px | 8.5px | 8px |
| 128×128 | 21.3px | 17px | 16px |
| 256×256 | 42.7px | 34px | 32px |
| 512×512 | 85.3px | 68px | 64px |

### 设计寓意
- **h形床架**: 表达"Bed"概念，床是休息之地，代码如床头灯般陪伴开发
- **双箭头>>**: 终端输出风格，表示持续输出代码
- **01000011**: 字母C的ASCII二进制，致敬Claude AI
- **Claude暖色背景**: #c26a4d，与Claude品牌呼应

### 输出文件
- `src-tauri/icons/icon.svg` - SVG源文件
- `src-tauri/icons/icon.png` - 512×512 PNG
- `src-tauri/icons/32x32.png` - Windows/Linux
- `src-tauri/icons/128x128.png` - macOS/iOS
- `src-tauri/icons/128x128@2x.png` - 高分屏
- `src-tauri/icons/256x256.png` - 大尺寸
- `src-tauri/icons/icon.ico` - Windows图标
- `src-tauri/icons/icon.icns` - macOS图标
- iOS/Android各尺寸变体