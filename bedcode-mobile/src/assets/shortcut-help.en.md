# Terminal Shortcut Reference

The mobile terminal simulates common desktop terminal shortcuts. Below is a description of each shortcut's function in the terminal.

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
