# Terminal Input Quick Guide

The mobile input bar bundles all convenience features: shortcut panel, custom shortcuts, custom commands, Agent CLI command presets, `/` completion and `@` file references. This guide covers them one by one.

## Input Bar Overview

The input bar sits at the bottom of the terminal, in three parts:

- **Quick Bar**: always-visible shortcuts and commands (auto-sorted by usage frequency), tap to send
- **Input box**: type your prompt; multi-line supported (expands to 3 lines on focus, max 6 lines)
- **Action buttons**: shortcut panel toggle, Send (text without Enter), Execute (text + Enter)

> Send vs Execute: **Send** puts the text into the terminal input line **without pressing Enter** (e.g. skill prefix completion); **Execute** sends the text plus Enter to run it immediately.

## Shortcut Panel

Tap the grid icon on the left of the input bar. Swipe **left/right** to switch between two pages:

| Page | Content |
|------|---------|
| Page 1 | General shortcuts + Enter/Del + arrow keys (keyboard layout) |
| Page 2 | Agent command presets (builtin, not deletable) + custom commands |

- Tap the dots at the bottom to jump between pages
- Swiping loops (right-swipe from page 1 goes to page 2 and vice versa)
- In landscape mode the panel is unavailable (shows a hint instead of silently ignoring)

### Shortcut Reference

| Key | Function |
|-----|----------|
| Tab | Auto-complete command, file or path |
| Enter | Execute current input |
| Esc | Interrupt running process / cancel |
| Del | Delete character before cursor |
| Ctrl+C | Send SIGINT interrupt signal |
| Ctrl+Z | Suspend current process (resume with `fg`) |
| Ctrl+L | Clear screen |
| ↑ ↓ ← → | History navigation / cursor movement |

## Custom Shortcuts

Tap the "shortcuts" button in the terminal header to open the **shortcut config dialog**, with two tabs:

- **List tab**: manage builtin/custom shortcuts — toggle visibility, delete custom ones, reset to defaults
- **Add tab**: capture a key combo by keyboard (tap the capture box and press keys) or build one from the key grid (modifiers + letters/numbers/function keys/edit keys/arrow keys)

New shortcuts take effect immediately and appear on page 1 and the Quick Bar.

## Custom Commands

On page 2 of the shortcut panel, tap **+** to add a custom command (just type the text):

- Added commands are listed together with presets; tap to execute (text + Enter)
- Edit mode (pencil button): delete custom commands (builtin presets are not deletable)
- Custom commands persist locally (settings DB), survive restarts
- Usage is tracked; frequent commands rise into the Quick Bar automatically

## Agent Command Presets

The first half of page 2 shows the **Agent CLI command set auto-detected for the current session**:

- Four preset sets: Claude Code / pi / Codex / OpenCode (12 commands each)
- Detection is based on the session config's launch command keywords (claude / pi / codex / opencode)
- If the agent is unrecognized (generic), page 2 shows only custom commands
- Presets are marked builtin and cannot be deleted; items in "send" mode (e.g. `/`, `/skill:`) fill the input line for you to complete before submitting

## `/` Command Completion

Typing `/` in the input box pops up a **local completion list** (reuses preset data, zero latency):

- Prefix filtering (e.g. `/co` → `/compact`, `/cost`)
- Tap an item to fill the input box, focus is kept — you decide to complete or send
- Not shown when the session agent is unrecognized (generic)
- The completion layer collapses automatically once you keep typing

## @ File References

In the file sidebar (header "files" button):

- **Tap a file**: open the file to view its content
- **Long-press a file**: copy the file path and auto-fill it into the input box as `@path` (the sidebar closes and the input box gains focus; a space is inserted when there is existing content)
- Agent CLIs (Claude Code etc.) resolve `@path` file references

## Interrupt & Cleanup

- **Interrupt generation**: press **Esc** on page 1 of the shortcut panel (same special-key channel)
- **Clear screen**: header clear button or Ctrl+L in the shortcut panel
- Interrupted draft: text stays in the input box until you confirm — nothing is lost on session switch

## FAQ

- **No Agent commands on page 2**: make sure the session config launch command contains claude / pi / codex / opencode keywords; when arriving via notification the session data takes a moment to load — wait briefly or re-enter
- **Panel overlapping the completion layer**: the panel closes automatically once you start typing
