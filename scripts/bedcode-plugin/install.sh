#!/bin/bash
# BedCode Plugin Installer
# Copy this plugin to ~/.claude/plugins/bedcode/

PLUGIN_SOURCE="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_TARGET="$HOME/.claude/plugins/bedcode"

echo "BedCode Plugin Installer"
echo "========================"
echo ""
echo "Source: $PLUGIN_SOURCE"
echo "Target: $PLUGIN_TARGET"
echo ""

# Create target directory
mkdir -p "$PLUGIN_TARGET"

# Copy files
cp -r "$PLUGIN_SOURCE/." "$PLUGIN_TARGET/"

# Make scripts executable
chmod +x "$PLUGIN_TARGET/scripts/"*.sh

echo "Plugin installed successfully!"
echo ""
echo "Next steps:"
echo "1. Make sure BedCode desktop app is running"
echo "2. In Claude Code, run: /bedcode on"
echo ""
echo "To uninstall, run:"
echo "   rm -rf $PLUGIN_TARGET"