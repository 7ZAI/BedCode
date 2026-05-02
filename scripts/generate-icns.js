import pngToIcns from 'png-to-icns';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateIcns() {
  const iconsDir = path.join(__dirname, '..', 'src-tauri', 'icons');

  // png-to-icns 需要 1024x1024 PNG
  const pngPath = path.join(iconsDir, 'ios', 'AppIcon-512@2x.png'); // 1024x1024

  if (!fs.existsSync(pngPath)) {
    console.error('Error: 1024x1024 PNG not found');
    return;
  }

  const pngBuffer = fs.readFileSync(pngPath);
  const icnsBuffer = await pngToIcns(pngBuffer);
  const icnsPath = path.join(iconsDir, 'icon.icns');
  fs.writeFileSync(icnsPath, icnsBuffer);
  console.log('  ✓ icon.icns (macOS icon)');
}

generateIcns().then(() => {
  console.log('\n✅ ICNS file generated!');
}).catch(console.error);