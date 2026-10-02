pub mod term;
pub mod window;

use crate::arcade::Arcade;
use crate::claude::{Signal, Watcher};

/// Feeds Claude status changes into the arcade. Returns the terminal app of the session that finished.
pub fn pump_claude(watcher: &mut Watcher, arcade: &mut Arcade) -> Option<String> {
    let mut done = None;
    for s in watcher.poll() {
        let Signal::Done { term } = s;
        done = Some(term);
    }
    arcade.claude_busy = watcher.busy();
    if done.is_some() {
        arcade.claude_done();
    }
    done
}
