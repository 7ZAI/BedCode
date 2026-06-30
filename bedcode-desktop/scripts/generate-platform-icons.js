import icongen from 'icon-gen';
import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function generateAllIcons() {
  const iconsDir = path.join(__dirname, '..', 'src-tauri', 'icons');
  const srcPng = path.join(iconsDir, 'icon.png'); // 512x512

  if (!fs.existsSync(srcPng)) {
    console.error('Error: icon.png (512x512) not found');
    return;
  }

  console.log('Generating platform icons from icon.png...\n');

  // Generate ICO
  const icoDir = iconsDir;
  await icongen(srcPng, icoDir, {
    report: true,
    icns: {},
    ico: {
      sizes: [16, 24, 32, 48, 64, 128, 256]
    }
  });

  console.log('\n✅ All platform icons generated!');
}

generateAllIcons().catch(console.error);