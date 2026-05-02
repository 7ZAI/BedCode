import sharp from 'sharp';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// 图标尺寸配置
const sizes = [
  { name: '32x32.png', size: 32 },
  { name: '64x64.png', size: 64 },
  { name: '128x128.png', size: 128 },
  { name: '128x128@2x.png', size: 256 },
  { name: '256x256.png', size: 256 },
  { name: 'icon.png', size: 512 },
];

// iOS图标尺寸
const iosSizes = [
  { name: 'AppIcon-20x20@1x.png', size: 20 },
  { name: 'AppIcon-20x20@2x.png', size: 40 },
  { name: 'AppIcon-20x20@2x-1.png', size: 40 },
  { name: 'AppIcon-20x20@3x.png', size: 60 },
  { name: 'AppIcon-29x29@1x.png', size: 29 },
  { name: 'AppIcon-29x29@2x.png', size: 58 },
  { name: 'AppIcon-29x29@2x-1.png', size: 58 },
  { name: 'AppIcon-29x29@3x.png', size: 87 },
  { name: 'AppIcon-40x40@1x.png', size: 40 },
  { name: 'AppIcon-40x40@2x.png', size: 80 },
  { name: 'AppIcon-40x40@2x-1.png', size: 80 },
  { name: 'AppIcon-40x40@3x.png', size: 120 },
  { name: 'AppIcon-60x60@2x.png', size: 120 },
  { name: 'AppIcon-60x60@3x.png', size: 180 },
  { name: 'AppIcon-76x76@1x.png', size: 76 },
  { name: 'AppIcon-76x76@2x.png', size: 152 },
  { name: 'AppIcon-83.5x83.5@2x.png', size: 167 },
  { name: 'AppIcon-512@2x.png', size: 1024 },
];

// Windows Store Logo尺寸
const storeSizes = [
  { name: 'StoreLogo.png', size: 50 },
  { name: 'Square30x30Logo.png', size: 30 },
  { name: 'Square44x44Logo.png', size: 44 },
  { name: 'Square71x71Logo.png', size: 71 },
  { name: 'Square89x89Logo.png', size: 89 },
  { name: 'Square107x107Logo.png', size: 107 },
  { name: 'Square142x142Logo.png', size: 142 },
  { name: 'Square150x150Logo.png', size: 150 },
  { name: 'Square284x284Logo.png', size: 284 },
  { name: 'Square310x310Logo.png', size: 310 },
];

// 创建SVG模板函数
function createSvg(size) {
  const scale = size / 120;
  const strokeWidth = 10 * scale;
  const fontSize = Math.max(4, size * 0.125); // 12.5%，最小4px
  const horizontalGap = size * 0.167; // 16.7%
  const verticalGap = size * 0.133; // 13.3%
  const cornerRadius = size * 0.219;

  // 计算path坐标（缩放）
  const p1x = 5 * scale;
  const p1y = 5 * scale;
  const p2x = 5 * scale;
  const p2y = 70 * scale;
  const p3x = 115 * scale;
  const p3y = 70 * scale;
  const p4x = 115 * scale;
  const p4y = 40 * scale;
  const p5x = 5 * scale;
  const p5y = 40 * scale;

  // 小尺寸图标简化代码显示
  let codeText = '&gt;&gt;01000011';
  if (size <= 32) {
    codeText = '&gt;&gt;01'; // 小图标只显示前几位
  }

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" rx="${cornerRadius}" fill="#c26a4d"/>
  <path d="M ${p1x} ${p1y} L ${p2x} ${p2y} L ${p3x} ${p3y} L ${p4x} ${p4y} L ${p5x} ${p5y}"
        stroke="white" stroke-width="${strokeWidth}"
        stroke-linecap="round" stroke-linejoin="round" fill="none"/>
  <text x="${horizontalGap}" y="${verticalGap + fontSize * 0.8}" fill="#22c55e" font-family="monospace" font-weight="600" font-size="${fontSize}">
    ${codeText}
  </text>
</svg>`;
}

async function generateIcons() {
  const iconsDir = path.join(__dirname, '..', 'src-tauri', 'icons');
  const iosDir = path.join(iconsDir, 'ios');

  // 确保目录存在
  if (!fs.existsSync(iosDir)) {
    fs.mkdirSync(iosDir, { recursive: true });
  }

  console.log('Generating BedCode icons...\n');

  // 生成主图标
  for (const { name, size } of sizes) {
    const svg = createSvg(size);
    const outputPath = path.join(iconsDir, name);
    await sharp(Buffer.from(svg)).png().toFile(outputPath);
    console.log(`  ✓ ${name} (${size}x${size})`);
  }

  // 生成iOS图标
  for (const { name, size } of iosSizes) {
    const svg = createSvg(size);
    const outputPath = path.join(iosDir, name);
    await sharp(Buffer.from(svg)).png().toFile(outputPath);
    console.log(`  ✓ ios/${name} (${size}x${size})`);
  }

  // 生成Windows Store图标
  for (const { name, size } of storeSizes) {
    const svg = createSvg(size);
    const outputPath = path.join(iconsDir, name);
    await sharp(Buffer.from(svg)).png().toFile(outputPath);
    console.log(`  ✓ ${name} (${size}x${size})`);
  }

  console.log('\n✅ All icons generated successfully!');
}

generateIcons().catch(console.error);