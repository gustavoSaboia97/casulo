//! Leaves the user's terminal usable after an agent exits.

use std::io::{IsTerminal, Write};

/// Turns off the terminal modes a full-screen agent switches on: mouse
/// reporting, focus events and bracketed paste, and shows the cursor again.
///
/// An agent killed before it cleans up (a double Ctrl+C, `docker stop`)
/// leaves them on, and the terminal keeps sending clicks to a program that
/// is gone, so text can no longer be selected or copied. Turning off a mode
/// that is already off does nothing.
const RESTORE_MODES: &str =
    "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1015l\x1b[?1004l\x1b[?2004l\x1b[?25h";

/// Restores the terminal on stderr when it is one, so a script reading
/// stdout never gets these codes.
pub fn restore_after_agent() {
    let mut stderr = std::io::stderr();
    if stderr.is_terminal() {
        // Nothing to do if this fails: the run is over either way.
        let _ = stderr.write_all(RESTORE_MODES.as_bytes());
        let _ = stderr.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_off_mouse_reporting_and_bracketed_paste() {
        for mode in ["?1000l", "?1002l", "?1003l", "?1006l", "?2004l"] {
            assert!(RESTORE_MODES.contains(mode), "{mode}");
        }
    }
}
