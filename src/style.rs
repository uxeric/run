//! Marks for the waiting prompt, the moment a paste starts running, and a stop.
//! The palette is the one the installer and the readme already use:
//! hot `#ff2a6d`, ice `#29f0ff`, amber `#f6c445`.
//! `NO_COLOR` and a pipe both stay plain.

const INVITATION: &str = "Paste commands. Ctrl-D runs them.";

pub struct Ink {
    on: bool,
}

impl Ink {
    pub fn on(on: bool) -> Self {
        Self { on }
    }

    /// The session tty. Callers only use this after they already have a terminal.
    pub fn tty() -> Self {
        Self::on(std::env::var_os("NO_COLOR").is_none())
    }

    /// Stderr. A pipe stays plain so logs and tests keep the uncolored text.
    pub fn stderr() -> Self {
        Self::on(
            std::env::var_os("NO_COLOR").is_none()
                && std::io::IsTerminal::is_terminal(&std::io::stderr()),
        )
    }

    pub fn enabled(&self) -> bool {
        self.on
    }

    pub fn hot(&self, text: &str) -> String {
        self.paint("255;42;109", text, false)
    }

    pub fn hot_bold(&self, text: &str) -> String {
        self.paint("255;42;109", text, true)
    }

    pub fn ice(&self, text: &str) -> String {
        self.paint("41;240;255", text, false)
    }

    pub fn amber(&self, text: &str) -> String {
        self.paint("246;196;69", text, false)
    }

    pub fn dim(&self, text: &str) -> String {
        if !self.on {
            return text.to_string();
        }
        format!("\x1b[2m{text}\x1b[0m")
    }

    fn paint(&self, rgb: &str, text: &str, bold: bool) -> String {
        if !self.on {
            return text.to_string();
        }
        let open = if bold { "1;38;2;" } else { "38;2;" };
        format!("\x1b[{open}{rgb}m{text}\x1b[0m")
    }
}

pub fn prompt_line(ink: &Ink) -> String {
    format!("{} {}", ink.hot_bold("❯"), ink.dim(INVITATION))
}

/// The line between an accepted paste and its output. It is the width of the
/// invitation, not the terminal, so a narrow window and a wide one match.
pub fn handoff(ink: &Ink) -> String {
    let columns = 2 + INVITATION.chars().count();
    ink.ice(&"─".repeat(columns))
}
