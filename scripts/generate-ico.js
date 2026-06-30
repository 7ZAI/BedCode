import pngToIco from 'png-to-ico';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateIco() {
  const iconsDir = path.join(__dirname, '..', 'src-tauri', 'icons');

  // 使用多个尺寸生成 ICO
  const sizes = [16, 32, 48, 64, 128, 256];
  const pngBuffers = [];

  for (const size of sizes) {
    const pngPath = path.join(iconsDir, size === 256 ? '256x256.png' : `${size}x${size}.png`);
    if (fs.existsSync(pngPath)) {
      pngBuffers.push(fs.readFileSync(pngPath));
    }
  }

  // 如果没有 16x16，使用 32x32 缩放
  if (!fs.existsSync(path.join(iconsDir, '16x16.png'))) {
    // 生成一个小的 16x16 PNG
    const sharp = (await import('sharp')).default;
    const svg16 = createSvg(16);
    const png16 = await sharp(Buffer.from(svg16)).png().toBuffer();
    pngBuffers.unshift(png16);
    fs.writeFileSync(path.join(iconsDir, '16x16.png'), png16);
    console.log('  ✓ 16x16.png (16x16)');
  }

  const icoBuffer = await pngToIco(pngBuffers);
  const icoPath = path.join(iconsDir, 'icon.ico');
  fs.writeFileSync(icoPath, icoBuffer);
  console.log('  ✓ icon.ico (Windows icon)');
}

function createSvg(size) {
  const scale = size / 120;
  const strokeWidth = 10 * scale;
  const fontSize = Math.max(2, size * 0.125);
  const horizontalGap = size * 0.167;
  const verticalGap = size * 0.133;
  const cornerRadius = size * 0.219;

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

  const codeText = size <= 20 ? '' : '&gt;&gt;01';

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  <rect width="${size}" height="${size}" rx="${cornerRadius}" fill="#c26a4d"/>
  <path d="M ${p1x} ${p1y} L ${p2x} ${p2y} L ${p3x} ${p3y} L ${p4x} ${p4y} L ${p5x} ${p5y}"
        stroke="white" stroke-width="${strokeWidth}"
        stroke-linecap="round" stroke-linejoin="round" fill="none"/>
  ${codeText ? `<text x="${horizontalGap}" y="${verticalGap + fontSize * 0.8}" fill="#22c55e" font-family="monospace" font-weight="600" font-size="${fontSize}">${codeText}</text>` : ''}
</svg>`;
}

generateIco().then(() => {
  console.log('\n✅ ICO file generated!');
}).catch(console.error);