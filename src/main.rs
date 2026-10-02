use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sidequest::arcade::{Arcade, Mode};
use sidequest::config::Config;
use sidequest::store::Store;
use sidequest::{claude, frontend, games, hooks, paths};

#[derive(Parser)]
#[command(name = "sidequest", version, about = "Games to play while Claude Code works")]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open the floating game window (the default).
    Window {
        /// Opened by the Claude hook: closes itself when Claude is done.
        #[arg(long)]
        auto: bool,
        /// Jump straight into a game (fathom, snake, invaders, 2048, mines).
        #[arg(long)]
        game: Option<String>,
    },
    /// Play in this terminal instead of a window.
    Play { game: Option<String> },
    /// Add, remove or check the Claude Code hooks in ~/.claude/settings.json.
    Hooks {
        #[arg(value_parser = ["install", "uninstall", "status"])]
        action: String,
    },
    /// Turn the automatic pop-up on.
    On,
    /// Turn the automatic pop-up off (you can still open the window yourself).
    Off,
    /// Show whether hooks are installed, Claude is busy, and the window is open.
    Status,
    /// Print the config path (`--init` writes a starter file).
    Config {
        #[arg(long)]
        init: bool,
    },
    /// Called by Claude Code hooks (start, stop, notify).
    #[command(hide = true)]
    Hook { event: String },
    #[command(name = "_pop", hide = true)]
    Pop { session: String, ms: u64 },
}

fn store() -> Store {
    Store::open(paths::state_dir())
}

fn check_game(id: &Option<String>) -> anyhow::Result<()> {
    if let Some(g) = id
        && games::find(g).is_none()
    {
        let ids: Vec<&str> = games::CATALOG.iter().map(|g| g.id).collect();
        anyhow::bail!("unknown game `{g}` (try: {})", ids.join(", "));
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Hook paths first: they must stay silent and never fail.
    match &cli.command {
        Some(Cmd::Hook { event }) => {
            hooks::run_hook(event);
            return ExitCode::SUCCESS;
        }
        Some(Cmd::Pop { session, ms }) => {
            hooks::run_pop(session, *ms);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cmd: Option<Cmd>) -> anyhow::Result<()> {
    let (cfg, warning) = Config::load(&paths::config_path());
    if let Some(w) = warning {
        eprintln!("warning: {w}");
    }
    match cmd.unwrap_or(Cmd::Window { auto: false, game: None }) {
        Cmd::Window { auto, game } => {
            check_game(&game)?;
            if hooks::window_running() {
                if !auto {
                    eprintln!("the sidequest window is already open");
                }
                return Ok(());
            }
            let mode = if auto && cfg.auto_close { Mode::Auto } else { Mode::Manual };
            frontend::window::run(Arcade::new(store(), mode, game.as_deref()), &cfg)
        }
        Cmd::Play { game } => {
            check_game(&game)?;
            use std::io::IsTerminal;
            if !std::io::stdout().is_terminal() {
                anyhow::bail!("`sidequest play` needs an interactive terminal");
            }
            frontend::term::run(Arcade::new(store(), Mode::Manual, game.as_deref()))
        }
        Cmd::Hooks { action } => {
            let settings = paths::claude_settings();
            match action.as_str() {
                "install" => {
                    let exe = std::env::current_exe()?.canonicalize()?;
                    hooks::install(&settings, &exe)?;
                    println!("Installed hooks in {} (backup: settings.json.sidequest-backup).", settings.display());
                    println!("The game window will pop up when Claude works for more than {}s.", cfg.delay_secs);
                }
                "uninstall" => {
                    if hooks::uninstall(&settings)? {
                        println!("Removed sidequest's hooks from {}.", settings.display());
                    } else {
                        println!("No sidequest hooks found.");
                    }
                }
                _ => println!("{}", if hooks::is_installed(&settings) { "installed" } else { "not installed" }),
            }
            Ok(())
        }
        c @ (Cmd::On | Cmd::Off) => {
            let on = matches!(c, Cmd::On);
            let p = paths::config_path();
            Config { enabled: on, ..cfg }.write(&p)?;
            println!("Pop-up {} ({})", if on { "on" } else { "off" }, p.display());
            Ok(())
        }
        Cmd::Status => {
            let installed = hooks::is_installed(&paths::claude_settings());
            let w = claude::Watcher::new(claude::events_path(), claude::now_ms());
            println!(
                "hooks:   {}",
                if installed { "installed" } else { "not installed (run `sidequest hooks install`)" }
            );
            println!("pop-up:  {} after {}s", if cfg.enabled { "on" } else { "off" }, cfg.delay_secs);
            println!(
                "claude:  {}",
                if w.busy() > 0 { format!("{} session(s) working", w.busy()) } else { "idle".into() }
            );
            println!("window:  {}", if hooks::window_running() { "open" } else { "closed" });
            Ok(())
        }
        Cmd::Config { init } => {
            let p = paths::config_path();
            if init && !p.exists() {
                cfg.write(&p)?;
                println!("wrote {}", p.display());
            } else {
                println!("{}", p.display());
            }
            Ok(())
        }
        Cmd::Hook { .. } | Cmd::Pop { .. } => Ok(()),
    }
}
