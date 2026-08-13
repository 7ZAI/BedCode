# Terminal Shortcut Reference

The mobile terminal simulates common desktop terminal shortcuts. Below is a description of each shortcut's function in the terminal.

## Input Bar

The terminal bottom is the input bar, which from bottom to top contains: input box, action buttons, and the quick bar.

### Quick Bar

- Always shows frequently used shortcuts and quick commands, automatically sorted by usage frequency (scroll horizontally for more)
- **Enter / Del are pinned to the far right**: they do not participate in frequency sorting and are always visible for thumb-friendly high-frequency access
- Tap to send the corresponding key/command to the terminal

### Input Box Action Buttons

| Button | Function |
|--------|----------|
| Sliders button | Toggle the shortcut panel (shortcuts, arrow keys and custom commands, swipe left/right to switch pages) |
| Send button (↑) | Send the input to the terminal input line (without Enter) |
| Execute button (paper plane) | Execute the input (appends Enter automatically, e.g. run a command) |

### File Sidebar (Referencing Files)

After opening the file tree from the terminal sidebar:

- **Tap a file**: open the file to view its content
- **Long-press a file**: copy the file path and auto-fill it into the input box as `@path` (no manual typing needed)

## Modifier Keys

| Shortcut | Function |
|----------|----------|
| Ctrl+C | Send SIGINT signal, interrupt the current running process |
| Ctrl+D | Send EOF (End of File), exit the current shell or end input |
| Ctrl+Z | Suspend the current process (send to background), use `fg` to resume |
| Ctrl+L | Clear screen, equivalent to the `clear` command |
| Ctrl+A | Move cursor to the beginning of the line |
| Ctrl+E | Move cursor to the end of the line |
| Ctrl+K | Delete from cursor to end of line |
| Ctrl+U | Delete from cursor to beginning of line |
| Ctrl+R | Reverse search through command history |

## Basic Keys

| Shortcut | Function |
|----------|----------|
| Tab | Auto-complete commands, filenames, or paths |
| Enter | Execute the current input command |
| Esc | Cancel current input / exit edit mode |
| Backspace | Delete the character before the cursor |

## Arrow Keys & Navigation

| Shortcut | Function |
|----------|----------|
| ↑ | Show previous command in history |
| ↓ | Show next command in history |
| ← | Move cursor left one character |
| → | Move cursor right one character |
| Home | Jump cursor to beginning of line |
| End | Jump cursor to end of line |
| Page Up | Scroll up to view terminal output |
| Page Down | Scroll down to view terminal output |

## Edit Keys

| Shortcut | Function |
|----------|----------|
| Insert | Toggle insert/overwrite mode |
| Delete | Delete the character after the cursor |

## Custom Shortcuts

In the shortcut configuration, you can add custom key combinations. Select a modifier key (Ctrl / Shift / Alt) plus a key to create one.

Custom shortcuts send the corresponding key sequence directly to the terminal. Their function depends on the currently running program.
