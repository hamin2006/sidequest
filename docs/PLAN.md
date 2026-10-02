# sidequest — plan

Games to play while Claude Code works. A floating, always-on-top window appears when Claude has been
busy for a few seconds, pauses and steps aside the moment Claude finishes, and puts you back in your
terminal. Claude's own terminal window is never touched, so scrollback, selection and search keep working.

## Pieces

```
Claude Code hooks ──► `sidequest hook start|stop|notify`   (fast, silent, reads hook JSON on stdin)
                          │ appends to events.log, tracks busy sessions
                          ▼
                 `sidequest _pop` (detached, waits `delay_secs`)
                          │ still busy and no window open?
                          ▼
                 `sidequest window --auto`  (eframe, always on top)
                          │ polls events.log; on stop/notify:
                          ▼  pause, "✓ Claude is done", save, close, refocus the terminal app
```

### Games (all in Rust, drawn with ratatui onto a character grid)
- **FATHOM**: the new sonar roguelike RPG (see `FATHOM.md`).
- **Snake**, **Space Invaders**, **2048**, **Minesweeper**.

Every game implements one trait: update with elapsed time and input, draw into a ratatui `Frame`,
save/load as JSON. That keeps them independent of where they're shown.

### Two frontends, one codebase
- **Window** (`sidequest window`): eframe/egui. Each frame the arcade draws into a ratatui
  `TestBackend` buffer, and the window paints that grid with Menlo. Always on top, focus on open.
- **Terminal** (`sidequest play`): the same games in any terminal through crossterm. Uses the kitty
  keyboard protocol where available (Ghostty) for real key-release events.

### Hooks
- `UserPromptSubmit` → `sidequest hook start`
- `Stop` → `sidequest hook stop`
- `Notification` → `sidequest hook notify` (permission prompts count as "Claude needs you")

Hooks must print nothing (UserPromptSubmit output is added to Claude's context), return in milliseconds,
and never fail loudly. `sidequest hooks install` merges these into `~/.claude/settings.json`, keeping
everything else and writing a backup first. `sidequest hooks uninstall` removes only its own entries.

### Rules
- Show the game only if a session has been busy for `delay_secs` (default 8): quick answers never flash a window.
- Any session finishing or asking for permission closes an auto-opened window. A window you opened
  yourself just pauses and shows the banner.
- If you close the window yourself while Claude is still busy, it stays closed until the next prompt.
- Focus returns to the terminal Claude runs in (`TERM_PROGRAM`: Ghostty, iTerm2, Terminal, VS Code, WezTerm…).

## Milestones
1. Core: input model, game trait, saves, launcher, terminal frontend, Snake, 2048.
2. Space Invaders, Minesweeper.
3. FATHOM: trench generation, sonar and echoes, the sub, HUD.
4. FATHOM: creatures and noise AI, salvage, pressure, battery, death and surfacing.
5. FATHOM: hub, workshop, research, bestiary, logs, relays, endings.
6. Window frontend.
7. Hooks, delayed pop-up, focus return, install/uninstall.
8. Hardening: tests for every game's rules, render tests at many sizes, hook and settings tests, README.
