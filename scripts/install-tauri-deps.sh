#!/bin/bash
# Install Tauri dependencies for Linux
# Run with: sudo ./scripts/install-tauri-deps.sh

set -e

echo "Installing Tauri development dependencies..."

# Update package list
apt-get update

# Install core dependencies
apt-get install -y \
    pkg-config \
    libgtk-3-dev \
    libgdk-pixbuf2.0-dev \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    libsoup-3.0-dev \
    javascriptcoregtk-4.1-dev

echo "✅ Tauri dependencies installed successfully!"
echo ""
echo "You can now run: cargo test"
