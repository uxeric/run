mod input;
mod style;
mod update;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const RUNNER: &str = include_str!("runner.sh");

const HELP: &str = "\
run — paste shell commands and run them in order

  run             Wait for a paste, then run it
  run --update    Download and install the latest version
  run --version   Show the installed version

A paste runs when it ends. Ctrl-D runs what you typed.
Comments (#) are ignored. Commands share one shell, so cd persists.
The run stops at the first command that exits non-zero.
A heredoc that is never closed is refused, and nothing runs.

Before the paste, run checks for a newer version. Enter installs it
and continues. n keeps this version.

RUN_NO_UPDATE_CHECK  Set to skip the update check.
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{} {error}", style::Ink::stderr().hot("run:"));
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args = parse_args()?;
    if args.version {
        let rev =
            update::installed_rev().unwrap_or_else(|| "not installed by install.sh".to_string());
        println!("run {} ({rev})", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    if args.update {
        let status = update::run_installer().map_err(|error| {
            format!("could not run the installer ({error}); nothing was updated.")
        })?;
        if !status.success() {
            return Err(
                "the update did not finish; your previous version is still installed.".to_string(),
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if args.help {
        print!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    }

    update::offer_if_newer()?;
    let script = input::read_script()?;
    let script = normalize(&script);
    if script.trim().is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    execute(&script)
}

struct Args {
    help: bool,
    version: bool,
    update: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut help = false;
    let mut version = false;
    let mut update = false;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => help = true,
            "-V" | "--version" => version = true,
            "--update" => update = true,
            other => {
                return Err(format!("unexpected argument `{other}`. Run `run --help`."));
            }
        }
    }
    Ok(Args {
        help,
        version,
        update,
    })
}

fn normalize(script: &str) -> String {
    let mut text = script.replace("\r\n", "\n").replace('\r', "\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn execute(script: &str) -> Result<ExitCode, String> {
    let files = TempFiles::create()?;
    write_private(&files.paste, script)?;
    write_private(&files.runner, RUNNER)?;

    let check = bash(&["--noprofile", "--norc", "-n"])
        .arg(&files.paste)
        .stdin(Stdio::null())
        .output()
        .map_err(bash_missing)?;
    let diagnostic =
        String::from_utf8_lossy(&check.stderr).replace(&files.paste.display().to_string(), "paste");
    if !check.status.success() || heredoc_left_open(&diagnostic) {
        let refused = style::Ink::stderr().amber("run: not running anything");
        eprint!("{refused}\n{diagnostic}");
        if !diagnostic.ends_with('\n') && !diagnostic.is_empty() {
            eprintln!();
        }
        return Ok(ExitCode::from(2));
    }

    // The seam means the paste was accepted. A refusal returns above, without it.
    show_handoff();

    let status = bash(&["--noprofile", "--norc"])
        .arg(&files.runner)
        .arg(&files.paste)
        .stdin(Stdio::inherit())
        .status()
        .map_err(bash_missing)?;
    Ok(exit_code(status))
}

fn show_handoff() {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return;
    }
    let Ok(mut tty) = File::options().write(true).open("/dev/tty") else {
        return;
    };
    let line = style::handoff(&style::Ink::tty());
    let _ = writeln!(tty, "{line}");
}

fn heredoc_left_open(diagnostic: &str) -> bool {
    diagnostic.contains("here-document") && diagnostic.contains("end-of-file")
}

fn bash(args: &[&str]) -> Command {
    let mut command = Command::new("bash");
    command.args(args);
    // BASH_ENV would source the login environment before the paste, including
    // with --noprofile --norc. SHELLOPTS and BASHOPTS would turn tracing on.
    command
        .env_remove("BASH_ENV")
        .env_remove("SHELLOPTS")
        .env_remove("BASHOPTS");
    command
}

fn bash_missing(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        "bash is not installed".into()
    } else {
        error.to_string()
    }
}

fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    if let Some(signal) = status.signal() {
        return ExitCode::from(128u8.saturating_add(u8::try_from(signal).unwrap_or(1)));
    }
    ExitCode::from(1)
}

fn write_private(path: &Path, text: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    Ok(())
}

struct TempFiles {
    paste: PathBuf,
    runner: PathBuf,
}

impl TempFiles {
    fn create() -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = env::temp_dir();
        let stem = format!("run-{}-{nanos}", std::process::id());
        Ok(Self {
            paste: dir.join(format!("{stem}.paste")),
            runner: dir.join(format!("{stem}.sh")),
        })
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.paste);
        let _ = fs::remove_file(&self.runner);
    }
}
