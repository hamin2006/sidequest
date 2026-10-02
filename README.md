# ◆ sidequest

Games to play while Claude Code works.

When Claude has been working on your prompt for a few seconds, a small always-on-top game window
pops up. The moment Claude finishes (or needs your permission), the game pauses and saves itself, the
window steps aside, and you're back in your terminal. Claude's own terminal is never touched, so
scrollback, selection and search all keep working.

## The games

| | |
| --- | --- |
| **FATHOM** | A new sonar roguelike RPG. Pilot a submersible down a pitch-black ocean trench. Every sonar ping lights up the walls and creatures around you for a few seconds, and tells everything that hunts by sound exactly where you are. Salvage wrecks, scan a bestiary, upgrade your sub, and find out what happened to the lost research sub *Meridian*. ([design doc](docs/FATHOM.md)) |
| **Snake** | Classic, with buffered turns and rising speed. |
| **Space Invaders** | Marching waves, crumbling shields, a bonus saucer. |
| **2048** | Slide and merge, with one undo. |
| **Minesweeper** | 16×16, 40 mines, safe first click, chording. |

Every game autosaves and resumes exactly where you left it, with a short countdown before real-time
games start moving again.

## Install

```bash
cargo install --path .          # or: cargo install --git https://github.com/hamin2006/sidequest
sidequest hooks install         # adds three hooks to ~/.claude/settings.json (backs it up first)
```

That's it. Next time Claude works for more than 8 seconds, the window appears.

## Commands

```bash
sidequest                 # open the game window yourself
sidequest play [game]     # play in this terminal instead
sidequest status          # hooks installed? Claude busy? window open?
sidequest off / on        # pause / resume the automatic pop-up
sidequest hooks uninstall # remove the hooks (leaves your other settings alone)
sidequest config --init   # write ~/.config/sidequest/config.toml
```

## FATHOM controls

| Key | |
| --- | --- |
| arrows / WASD | move (shift: boost, loud) |
| space | sonar ping |
| f | lamp on/off |
| q | drop a decoy |
| e | salvage a wreck hatch ▣ |
| h | harpoon (once upgraded) |
| r | autopilot back to the surface |
| tab | chart of this dive |
| ? | in-game help |

In every game, `esc` pauses (resume, restart or go back to the games list).

## Configuration

`~/.config/sidequest/config.toml`, all optional:

```toml
enabled = true          # automatic pop-up
delay_secs = 8.0        # only pop up if Claude is still working after this long
auto_close = true       # close the window when Claude finishes (false: just pause it)
focus_terminal = true   # bring your terminal back to the front afterwards
always_on_top = true
font_size = 15.0
cols = 104              # window size in character cells
rows = 34
```

## How it works

```
Claude Code ── UserPromptSubmit ─► sidequest hook start ─► events.log + detached `_pop`
            ── Stop / Notification ─► sidequest hook stop|notify ─► events.log
_pop: waits delay_secs; if that prompt is still running and no window is open → `sidequest window --auto`
window: tails events.log; on stop/notify → pause, "✓ Claude is done", save, close, re-focus the terminal
```

- Hooks print nothing (UserPromptSubmit output would otherwise land in Claude's context), always exit
  0, and take about 5 ms.
- Quick answers never flash a window. Close the window yourself and it stays closed for that prompt.
- Several Claude sessions at once are tracked separately; any one finishing brings you back.
- Every game is drawn with ratatui. The window renders that character grid with egui and Menlo, and
  `sidequest play` renders the same thing in a terminal.

## Development

```bash
cargo test     # game rules, FATHOM world generation and AI, rendering at many sizes, hooks, CLI
cargo clippy --all-targets -- -D warnings
```

`SIDEQUEST_STATE_DIR`, `SIDEQUEST_CONFIG` and `SIDEQUEST_CLAUDE_SETTINGS` redirect all files (used by
the tests). For debugging the window, `SIDEQUEST_SCRIPT="0.5:enter,1:space"` injects keys and
`SIDEQUEST_SNAPSHOT=out.bmp` saves a screenshot of the window and exits.

## License

MIT
