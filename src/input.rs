//! Read a pasted script. On a terminal, show the prompt, echo the paste with
//! its newlines, and return when the paste ends or the user presses Ctrl-D.
//! A pipe is read until EOF.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use libc::termios;

pub fn read_script() -> Result<String, String> {
    if !io::stdin().is_terminal() {
        let mut script = String::new();
        io::stdin()
            .read_to_string(&mut script)
            .map_err(|error| error.to_string())?;
        return Ok(script);
    }
    let mut term = Term::activate().map_err(|error| error.to_string())?;
    let script = term.read_paste().map_err(|error| error.to_string())?;
    Ok(script)
}

struct Term {
    tty: File,
    fd: i32,
    column: usize,
}

impl Term {
    fn activate() -> io::Result<Self> {
        let tty = File::options().read(true).write(true).open("/dev/tty")?;
        let fd = io::stdin().as_raw_fd();
        let orig = read_termios(fd)?;
        publish(fd, orig);
        install_handlers();
        // Mark the session active before leaving cooked mode so a signal in
        // this window still restores the saved settings.
        SLOT.active.store(true, Ordering::SeqCst);
        if !apply_cbreak(fd, &orig) {
            disarm();
            return Err(io::Error::last_os_error());
        }
        let mut term = Self { tty, fd, column: 0 };
        term.tty.write_all(b"\x1b[?2004h")?;
        term.tty.flush()?;
        write_hint(&mut term.tty)?;
        Ok(term)
    }

    fn read_paste(&mut self) -> io::Result<String> {
        let mut script: Vec<u8> = Vec::new();
        let mut escape: Vec<u8> = Vec::new();
        let mut in_escape = false;
        let mut in_paste = false;
        let mut saw_cr = false;
        let mut chunk = [0u8; 1024];

        loop {
            let read = unsafe { libc::read(self.fd, chunk.as_mut_ptr().cast(), chunk.len()) };
            if read == 0 {
                break;
            }
            if read < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            for &byte in &chunk[..read as usize] {
                if in_escape {
                    escape.push(byte);
                    match take_escape(&escape) {
                        Escape::More => {}
                        Escape::Ignore => {
                            in_escape = false;
                            escape.clear();
                        }
                        Escape::PasteStart => {
                            in_paste = true;
                            in_escape = false;
                            escape.clear();
                        }
                        Escape::PasteEnd => {
                            in_paste = false;
                            in_escape = false;
                            escape.clear();
                            if !script.is_empty() {
                                return Ok(bytes_to_string(script));
                            }
                        }
                    }
                    continue;
                }

                if byte == 0x1b {
                    in_escape = true;
                    escape.clear();
                    escape.push(byte);
                    continue;
                }
                if byte == 0x04 && !in_paste {
                    break;
                }
                if byte == 0x15 && !in_paste {
                    kill_line(&mut self.tty, &mut script, &mut self.column)?;
                    continue;
                }
                if (byte == 0x7f || byte == 0x08) && !in_paste {
                    backspace(&mut self.tty, &mut script, &mut self.column)?;
                    continue;
                }
                if byte == b'\r' {
                    saw_cr = true;
                    push_newline(&mut self.tty, &mut script, &mut self.column)?;
                    continue;
                }
                if byte == b'\n' {
                    if saw_cr {
                        saw_cr = false;
                        continue;
                    }
                    push_newline(&mut self.tty, &mut script, &mut self.column)?;
                    continue;
                }
                saw_cr = false;
                push_byte(&mut self.tty, &mut script, &mut self.column, byte)?;
            }
            self.tty.flush()?;
            if script_ends_input(&chunk[..read as usize], in_paste) {
                break;
            }
        }
        Ok(bytes_to_string(script))
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        // Ctrl-D can leave the cursor mid-line. Finish that line before cooked
        // mode returns, or the seam and the shell prompt continue the typed text.
        if self.column != 0 {
            let _ = self.tty.write_all(b"\n");
            let _ = self.tty.flush();
        }
        disarm();
    }
}

fn write_hint(tty: &mut File) -> io::Result<()> {
    let line = crate::style::prompt_line(&crate::style::Ink::tty());
    tty.write_all(line.as_bytes())?;
    tty.write_all(b"\n")?;
    tty.flush()
}

fn push_newline(tty: &mut File, script: &mut Vec<u8>, column: &mut usize) -> io::Result<()> {
    script.push(b'\n');
    *column = 0;
    tty.write_all(b"\n")
}

fn push_byte(tty: &mut File, script: &mut Vec<u8>, column: &mut usize, byte: u8) -> io::Result<()> {
    script.push(byte);
    // Count one column per Unicode scalar, not per continuation byte.
    if byte & 0b1100_0000 != 0b1000_0000 {
        *column += 1;
    }
    tty.write_all(&[byte])
}

fn backspace(tty: &mut File, script: &mut Vec<u8>, column: &mut usize) -> io::Result<()> {
    if *column == 0 || script.last() == Some(&b'\n') {
        return Ok(());
    }
    pop_char(script);
    *column = column.saturating_sub(1);
    tty.write_all(b"\x08 \x08")
}

fn kill_line(tty: &mut File, script: &mut Vec<u8>, column: &mut usize) -> io::Result<()> {
    while *column > 0 && script.last() != Some(&b'\n') {
        backspace(tty, script, column)?;
    }
    Ok(())
}

fn pop_char(script: &mut Vec<u8>) {
    if script.is_empty() || script.last() == Some(&b'\n') {
        return;
    }
    script.pop();
    while script
        .last()
        .is_some_and(|byte| byte & 0b1100_0000 == 0b1000_0000)
    {
        script.pop();
    }
}

/// Ctrl-D is detected inside the byte loop. This catches it when the byte that
/// ended the read was Ctrl-D outside a paste, so the outer loop stops too.
fn script_ends_input(chunk: &[u8], in_paste: bool) -> bool {
    !in_paste && chunk.contains(&0x04)
}

enum Escape {
    More,
    Ignore,
    PasteStart,
    PasteEnd,
}

/// A CSI sequence is ESC `[`, then parameter bytes, then a final byte in `@`–`~`.
/// The `[` itself is not the final byte. Paste markers are CSI `200~` and `201~`.
fn take_escape(bytes: &[u8]) -> Escape {
    if bytes.len() < 2 {
        return Escape::More;
    }
    if bytes[1] != b'[' {
        return Escape::Ignore;
    }
    if bytes.len() == 2 {
        return Escape::More;
    }
    let last = *bytes.last().unwrap();
    if !(0x40..=0x7e).contains(&last) {
        return if bytes.len() > 32 {
            Escape::Ignore
        } else {
            Escape::More
        };
    }
    if bytes == b"\x1b[200~" {
        Escape::PasteStart
    } else if bytes == b"\x1b[201~" {
        Escape::PasteEnd
    } else {
        Escape::Ignore
    }
}

fn bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

fn read_termios(fd: i32) -> io::Result<termios> {
    let mut orig: termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(orig)
}

fn apply_cbreak(fd: i32, orig: &termios) -> bool {
    let mut cbreak = *orig;
    cbreak.c_lflag &= !(libc::ICANON | libc::ECHO | libc::IEXTEN);
    // ICRNL would turn a pasted CR LF into two newlines. Enter arrives as CR.
    cbreak.c_iflag &= !(libc::ICRNL | libc::INLCR | libc::IXON | libc::IXOFF);
    cbreak.c_oflag |= libc::OPOST | libc::ONLCR;
    cbreak.c_cc[libc::VMIN] = 1;
    cbreak.c_cc[libc::VTIME] = 0;
    unsafe { libc::tcsetattr(fd, libc::TCSANOW, &cbreak) == 0 }
}

struct Slot {
    fd: AtomicI32,
    active: AtomicBool,
    orig: std::cell::UnsafeCell<termios>,
}

// orig is written on the main thread before `active` is set, and only read
// after that. The atomics publish the fd and the active flag.
unsafe impl Sync for Slot {}

static SLOT: Slot = Slot {
    fd: AtomicI32::new(-1),
    active: AtomicBool::new(false),
    orig: std::cell::UnsafeCell::new(unsafe { std::mem::zeroed() }),
};

fn publish(fd: i32, orig: termios) {
    unsafe { *SLOT.orig.get() = orig };
    SLOT.fd.store(fd, Ordering::SeqCst);
}

fn disarm() {
    if !SLOT.active.swap(false, Ordering::SeqCst) {
        uninstall_handlers();
        return;
    }
    let fd = SLOT.fd.load(Ordering::SeqCst);
    unsafe {
        libc::tcsetattr(fd, libc::TCSANOW, SLOT.orig.get());
        let off = b"\x1b[?2004l";
        libc::write(fd, off.as_ptr().cast(), off.len());
    }
    uninstall_handlers();
}

fn install_handlers() {
    unsafe {
        libc::signal(libc::SIGINT, on_fatal as *const () as usize);
        libc::signal(libc::SIGTERM, on_fatal as *const () as usize);
        libc::signal(libc::SIGQUIT, on_fatal as *const () as usize);
        libc::signal(libc::SIGHUP, on_fatal as *const () as usize);
        libc::signal(libc::SIGTSTP, on_stop as *const () as usize);
    }
}

fn uninstall_handlers() {
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
        libc::signal(libc::SIGQUIT, libc::SIG_DFL);
        libc::signal(libc::SIGHUP, libc::SIG_DFL);
        libc::signal(libc::SIGTSTP, libc::SIG_DFL);
    }
}

extern "C" fn on_fatal(sig: i32) {
    disarm();
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

extern "C" fn on_stop(_: i32) {
    let fd = SLOT.fd.load(Ordering::SeqCst);
    disarm();
    unsafe {
        libc::signal(libc::SIGTSTP, libc::SIG_DFL);
        libc::raise(libc::SIGTSTP);
        install_handlers();
        let orig = *SLOT.orig.get();
        SLOT.active.store(true, Ordering::SeqCst);
        apply_cbreak(fd, &orig);
        let on = b"\x1b[?2004h";
        libc::write(fd, on.as_ptr().cast(), on.len());
    }
}
