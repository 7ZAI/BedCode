# BedCode Desktop - Linux Build Dependencies

## System Dependencies (Ubuntu/Debian)

### Build-time Dependencies
```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libsqlite3-dev \
  pkg-config \
  build-essential \
  curl \
  wget \
  file \
  desktop-file-utils
```

### Runtime Dependencies (auto-installed via DEB package)
- `libwebkit2gtk-4.1-0` - WebView2 rendering engine
- `libgtk-3-0` - GTK3 for dialogs and UI
- `libayatana-appindicator3-1` - System tray support (AppIndicator)
- `librsvg2-common` - SVG icon support

## Rust Toolchain
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Add targets for cross-compilation (if building on x86_64 for ARM64)
rustup target add aarch64-unknown-linux-gnu
# Requires: sudo apt install -y gcc-aarch64-linux-gnu libc6-dev-arm64-cross
```

## Node.js / pnpm
```bash
# Install Node.js 20+
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs

# Install pnpm
corepack enable
corepack prepare pnpm@latest --activate
```

## Build Commands

### Development
```bash
cd bedcode-desktop
pnpm install
pnpm run tauri:dev
```

### Production Build (DEB)
```bash
cd bedcode-desktop
pnpm install
pnpm run tauri:build
# Output: src-tauri/target/release/bundle/deb/bedcode_<version>_<arch>.deb
```

### Cross-compilation for ARM64 (on x86_64)
```bash
cd bedcode-desktop
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
pnpm run tauri:build -- --target aarch64-unknown-linux-gnu
```

## Packaging Details

The DEB package is configured with:
- **Package name**: `bedcode`
- **Section**: `utils`
- **Priority**: `optional`
- **Architectures**: `x86_64`, `aarch64`
- **Desktop entry**: `resources/deb/bedcode.desktop`
- **Icon**: `icons/icon.svg` (scalable vector)

## Troubleshooting

### WebKitGTK not found
```bash
sudo apt install libwebkit2gtk-4.1-dev
```

### AppIndicator not working (tray icon missing)
```bash
sudo apt install libayatana-appindicator3-dev
# For GNOME Shell, also install: gnome-shell-extension-appindicator
```

### SVG icons not rendering
```bash
sudo apt install librsvg2-dev librsvg2-common
```

### Linker errors on cross-compilation
Ensure cross-compilation toolchain is installed:
```bash
sudo apt install gcc-aarch64-linux-gnu libc6-dev-arm64-cross
```

## CI/CD (GitHub Actions)

The release workflow (`.github/workflows/release.yml`) should be updated to include Linux builds:

```yaml
jobs:
  build-linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install dependencies
        run: |
          sudo apt update
          sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
      - name: Install pnpm
        run: corepack enable && corepack prepare pnpm@latest --activate
      - name: Build DEB
        run: |
          cd bedcode-desktop
          pnpm install
          pnpm run tauri:build
      - name: Upload DEB artifact
        uses: actions/upload-artifact@v4
        with:
          name: bedcode-deb
          path: bedcode-desktop/src-tauri/target/release/bundle/deb/*.deb
```

## Notes

1. **Updater**: The DEB package uses the same updater endpoint as Windows (GitHub releases). The updater will download and install updates automatically.

2. **System Tray**: Requires AppIndicator support. On GNOME, users may need to install `gnome-shell-extension-appindicator` extension.

3. **Wayland**: Tauri 2.x supports Wayland via WebKitGTK. If issues occur, set `GDK_BACKEND=x11` as fallback.

4. **File Associations**: The `.desktop` file registers the app in application menus. No additional MIME type registration is needed for this app.