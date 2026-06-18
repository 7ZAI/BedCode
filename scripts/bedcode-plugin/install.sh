#!/bin/bash
# BedCode Plugin Installer

set -e

PLUGIN_DIR="$HOME/.claude/plugins/bedcode"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "Installing BedCode plugin..."

# Remove old installation
if [ -d "$PLUGIN_DIR" ]; then
    echo "Removing old installation..."
    rm -rf "$PLUGIN_DIR"
fi

# Create plugin directory
mkdir -p "$PLUGIN_DIR"

# Copy plugin files
cp -r "$SCRIPT_DIR/.claude-plugin" "$PLUGIN_DIR/"
cp -r "$SCRIPT_DIR/hooks" "$PLUGIN_DIR/"
cp -r "$SCRIPT_DIR/scripts" "$PLUGIN_DIR/"
cp -r "$SCRIPT_DIR/commands" "$PLUGIN_DIR/"

# Make scripts executable
chmod +x "$PLUGIN_DIR/scripts/"*.sh 2>/dev/null || true

echo "Plugin installed to: $PLUGIN_DIR"
echo ""
echo "Restart Claude Code to load the plugin."
echo "Use /bedcode status to view session events."