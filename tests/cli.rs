use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_run")
}

fn run_with(args: &[&str], input: &str, envs: &[(&str, &str)]) -> Output {
    let mut child = Command::new(bin());
    child
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        child.env(key, value);
    }
    let mut child = child.spawn().expect("spawn run");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    child.wait_with_output().expect("wait for run")
}

fn run(input: &str) -> (String, String, i32) {
    let output = run_with(&[], input, &[]);
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(1),
    )
}

fn scratch_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("run-test-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn runs_commands_in_order_and_ignores_comments() {
    let (stdout, stderr, code) = run("\
# skip this line
echo one
echo two                # expect two, and do not run the note
# echo hidden
echo three
");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "one\ntwo\nthree\n");
    assert_eq!(stderr, "");
}

#[test]
fn keeps_hashes_in_words_quotes_and_heredocs() {
    let (stdout, stderr, code) = run("\
echo foo#bar # trailing
echo 'keep # this'
cat <<'EOF'
Adds Platonic sign-in, MAM access per site, and a console in plain terms
body # keep
EOF
");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        stdout,
        "\
foo#bar
keep # this
Adds Platonic sign-in, MAM access per site, and a console in plain terms
body # keep
"
    );
    assert_eq!(stderr, "");
}

#[test]
fn directory_change_persists_for_later_commands() {
    let dir = scratch_dir();
    let script = format!("cd {}\npwd\nprintf '%s\\n' \"$PWD\"\n", dir.display());
    let (stdout, stderr, code) = run(&script);
    assert_eq!(code, 0, "{stderr}");
    let expected = format!("{}\n{}\n", dir.display(), dir.display());
    assert_eq!(stdout, expected);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stops_at_the_first_failing_command() {
    let (stdout, stderr, code) = run("echo one\nfalse\necho two\n");
    assert_eq!(code, 1);
    assert_eq!(stdout, "one\n");
    assert!(stderr.contains("run: exited 1\nfalse\n"), "{stderr}");
    assert!(!stdout.contains("two"));
}

#[test]
fn stops_when_a_pipeline_fails() {
    let (stdout, stderr, code) = run("echo one\nfalse | true\necho two\n");
    assert_eq!(code, 1, "{stderr}");
    assert_eq!(stdout, "one\n");
    assert!(stderr.contains("run: exited 1\n"), "{stderr}");
    assert!(!stdout.contains("two"));
}

#[test]
fn refuses_an_unclosed_heredoc_before_running_anything() {
    let (stdout, stderr, code) = run("echo RAN\ncat <<'EOF'\nhello\n");
    assert_eq!(code, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("not running anything"), "{stderr}");
    assert!(stderr.contains("here-document"), "{stderr}");
    assert!(!stdout.contains("RAN"));
    assert!(!stdout.contains("hello"));
}

#[test]
fn refuses_an_unclosed_quote_before_running_anything() {
    let (stdout, stderr, code) = run("echo RAN\necho 'hello\n");
    assert_eq!(code, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("not running anything"), "{stderr}");
    assert!(stderr.contains("unexpected EOF"), "{stderr}");
}

#[test]
fn does_not_source_bash_env() {
    let dir = scratch_dir();
    let env_file = dir.join("env.sh");
    fs::write(&env_file, "echo STARTUP_RAN\n").unwrap();
    let output = run_with(
        &[],
        "echo hello\n",
        &[("BASH_ENV", env_file.to_str().unwrap())],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert_eq!(stdout, "hello\n");
    assert!(!stdout.contains("STARTUP_RAN"), "{stdout}");
    assert!(!stderr.contains("STARTUP_RAN"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn normalizes_crlf_and_runs_the_last_line_without_a_newline() {
    let (stdout, stderr, code) = run("echo one\r\necho two");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "one\ntwo\n");
}

#[test]
fn empty_and_comment_only_pastes_do_nothing() {
    let (stdout, stderr, code) = run("");
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "");

    let (stdout, stderr, code) = run("# only a note\n\n  # another\n");
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "");
    assert_eq!(stderr, "");
}

#[test]
fn version_prints_without_running_a_paste() {
    let output = run_with(&["--version"], "echo SHOULD_NOT\n", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(stdout.contains("run 0.1.0"), "{stdout}");
    assert!(stdout.contains("not installed by install.sh"), "{stdout}");
    assert!(!stdout.contains("SHOULD_NOT"), "{stdout}");
}

#[test]
fn help_does_not_run_the_paste() {
    let output = run_with(&["--help"], "echo SHOULD_NOT\n", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(stdout.contains("Ctrl-D"), "{stdout}");
    assert!(!stdout.contains("SHOULD_NOT"), "{stdout}");
}

#[test]
fn unknown_argument_is_rejected() {
    let output = run_with(&["--nope"], "", &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("unexpected argument"), "{stderr}");
}

fn pty(mode: &str) -> (Vec<u8>, i32) {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pty_paste.py");
    let output = Command::new("python3")
        .arg(script)
        .arg(bin())
        .arg(mode)
        .output()
        .expect("python pty driver");
    (
        output.stdout,
        output.status.code().unwrap_or_else(|| {
            panic!(
                "pty driver failed\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        }),
    )
}

#[test]
fn paste_appears_on_its_own_lines_and_then_runs() {
    let (shown, code) = pty("paste");
    let text = String::from_utf8_lossy(&shown);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("❯ Paste commands. Ctrl-D runs them."),
        "{text}"
    );
    assert!(
        !text.contains("\u{1b}[38;2;"),
        "NO_COLOR still colored the session:\n{text}"
    );
    assert!(!text.contains("\u{1b}[200~"), "{text}");
    assert!(!text.contains("\u{1b}[201~"), "{text}");
    let first = text.find("printf 'OUT_A").expect("first pasted line");
    let second = text.find("printf 'OUT_B").expect("second pasted line");
    assert!(first < second, "{text}");
    assert!(
        text[first..second].contains('\n'),
        "paste did not keep the line break:\n{text}"
    );
    let seam = text
        .find('─')
        .unwrap_or_else(|| panic!("missing seam:\n{text}"));
    let output = text
        .find("OUT_A\r\n")
        .or_else(|| text.find("OUT_A\n"))
        .unwrap_or_else(|| panic!("missing output:\n{text}"));
    assert!(second < seam && seam < output, "{text}");
    assert_eq!(text.matches("NO_RUN").count(), 1, "{text}");
    assert!(
        text.contains("OUT_A\r\n") || text.contains("OUT_A\n"),
        "{text}"
    );
    assert!(
        text.contains("OUT_B\r\n") || text.contains("OUT_B\n"),
        "{text}"
    );
}

#[test]
fn ctrl_d_runs_typed_lines() {
    let (shown, code) = pty("ctrld");
    let text = String::from_utf8_lossy(&shown);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("OUT_C\r\n") || text.contains("OUT_C\n"),
        "{text}"
    );
}

#[test]
fn the_prompt_the_seam_and_a_failed_command_use_the_night_city_colors() {
    let (shown, code) = pty("color");
    let text = String::from_utf8_lossy(&shown);
    assert_eq!(code, 1, "{text}");
    assert!(
        text.contains("\u{1b}[1;38;2;255;42;109m❯"),
        "prompt mark:\n{text}"
    );
    let invitation = "them.";
    let invited = text
        .find(invitation)
        .map(|at| at + invitation.len())
        .unwrap_or_else(|| panic!("missing invitation:\n{text}"));
    let echo = text
        .find("printf 'OUT_E")
        .unwrap_or_else(|| panic!("missing echo:\n{text}"));
    assert!(
        text[invited..echo].contains("\u{1b}[0m"),
        "typed text kept the prompt color:\n{text}"
    );
    let seam = text
        .find("\u{1b}[38;2;41;240;255m─")
        .unwrap_or_else(|| panic!("seam:\n{text}"));
    let seam_end = text
        .find("─\u{1b}[0m")
        .unwrap_or_else(|| panic!("seam did not reset:\n{text}"));
    let output = text
        .find("OUT_E\r\n")
        .or_else(|| text.find("OUT_E\n"))
        .unwrap_or_else(|| panic!("missing output:\n{text}"));
    let failed = text
        .find("\u{1b}[38;2;255;42;109mrun: exited 1")
        .unwrap_or_else(|| panic!("status:\n{text}"));
    assert!(
        echo < seam && seam < seam_end && seam_end < output && output < failed,
        "{text}"
    );
}

#[test]
fn a_refused_paste_draws_no_seam() {
    let (shown, code) = pty("refuse");
    let text = String::from_utf8_lossy(&shown);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("not running anything"), "{text}");
    assert!(!text.contains('─'), "{text}");
}

#[test]
fn backspace_edits_the_line_before_it_runs() {
    let (shown, code) = pty("backspace");
    let text = String::from_utf8_lossy(&shown);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("OUT_D"), "{text}");
    assert!(!text.contains("ZOUT_D"), "{text}");
}

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const GIT_FAKE: &str = r#"#!/bin/bash
if [[ ${1-} == rev-parse && ${2-} == --show-toplevel ]]; then
  pwd
  exit 0
fi
if [[ ${1-} == rev-parse && ${2-} == --abbrev-ref ]]; then
  cat "$PWD/.fake-branch"
  exit 0
fi
if [[ ${1-} == rev-parse && ${2-} == HEAD ]]; then
  cat "$PWD/.fake-head"
  exit 0
fi
if [[ ${1-} == remote ]]; then
  printf 'origin\n'
  exit 0
fi
if [[ ${1-} == status ]]; then
  printf 'CLEAN\n'
  exit 0
fi
if [[ ${1-} == push ]]; then
  if [[ -f $PWD/.fake-push-fail ]]; then
    exit 1
  fi
  exit 0
fi
exit 0
"#;

const GH_FAKE: &str = r#"#!/bin/bash
printf '%s\n' "$*" >> "$FAKE_GH_DIR/calls"
if [[ ${1-} == run && ${2-} == list ]]; then
  sha=""
  prev=""
  for arg in "$@"; do
    if [[ $prev == --commit ]]; then
      sha=$arg
    fi
    prev=$arg
  done
  count_file="$FAKE_GH_DIR/count-$sha"
  count=0
  if [[ -f $count_file ]]; then
    count=$(cat "$count_file")
  fi
  count=$((count + 1))
  printf '%s\n' "$count" > "$count_file"
  file="$FAKE_GH_DIR/$sha/$count"
  if [[ ! -f $file ]]; then
    file="$FAKE_GH_DIR/$sha/body"
  fi
  if [[ -f $file ]]; then
    cat "$file"
  fi
  exit 0
fi
if [[ ${1-} == run && ${2-} == watch ]]; then
  printf '%s\n' "$*" >> "$FAKE_GH_DIR/watch"
  if [[ -f $FAKE_GH_DIR/fail-id ]]; then
    fail=$(cat "$FAKE_GH_DIR/fail-id")
    for arg in "$@"; do
      if [[ $arg == "$fail" ]]; then
        exit 1
      fi
    done
  fi
  exit 0
fi
exit 0
"#;

struct Stage {
    root: PathBuf,
    data: PathBuf,
}

impl Stage {
    fn new() -> Self {
        let root = scratch_dir();
        let bin = root.join("bin");
        let data = root.join("data");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&data).unwrap();
        write_exe(&bin.join("git"), GIT_FAKE);
        write_exe(&bin.join("gh"), GH_FAKE);
        Self { root, data }
    }

    fn repo(&self, name: &str, sha: &str, branch: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".fake-head"), format!("{sha}\n")).unwrap();
        fs::write(dir.join(".fake-branch"), format!("{branch}\n")).unwrap();
        dir
    }

    fn runs(&self, sha: &str, name: &str, json: &str) {
        let dir = self.data.join(sha);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), json).unwrap();
    }

    fn fail_watch(&self, id: &str) {
        fs::write(self.data.join("fail-id"), id).unwrap();
    }

    fn run_paste(&self, script: &str) -> (String, String, i32) {
        let path = format!(
            "{}:{}",
            self.root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let data = self.data.display().to_string();
        let output = run_with(
            &[],
            script,
            &[
                ("PATH", &path),
                ("FAKE_GH_DIR", &data),
                ("RUN_WATCH_INTERVAL", "0"),
                ("RUN_WATCH_TRIES", "4"),
            ],
        );
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(1),
        )
    }

    fn watch_log(&self) -> String {
        fs::read_to_string(self.data.join("watch")).unwrap_or_default()
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_exe(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

#[test]
fn bare_watch_after_a_tag_push_waits_for_that_tag_and_stops_when_it_fails() {
    let stage = Stage::new();
    let repo = stage.repo("client", SHA_A, "master");
    stage.runs(
        SHA_A,
        "1",
        r#"[{"databaseId":444,"createdAt":"2099-01-01T00:00:00Z","headBranch":"master"},{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"v0.2.0"}]"#,
    );
    stage.runs(
        SHA_A,
        "2",
        r#"[{"databaseId":444,"createdAt":"2099-01-01T00:00:00Z","headBranch":"master"},{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"v0.2.0"},{"databaseId":222,"createdAt":"2099-01-01T00:00:00Z","headBranch":"v0.2.0"}]"#,
    );
    stage.fail_watch("222");
    let script = format!(
        "cd {}\ngit push origin v0.2.0\ngh run watch --compact\necho AFTER\n",
        repo.display()
    );
    let (stdout, stderr, code) = stage.run_paste(&script);
    assert_eq!(code, 1, "{stderr}");
    assert!(!stdout.contains("AFTER"), "{stdout}");
    assert_eq!(stage.watch_log(), "run watch 222 --compact --exit-status\n");
    assert!(stderr.contains("run: exited 1"), "{stderr}");
    assert!(stderr.contains("gh run watch"), "{stderr}");
}

#[test]
fn a_named_watch_keeps_its_run_and_a_failed_watch_stops_the_paste() {
    let stage = Stage::new();
    stage.fail_watch("222");
    let (stdout, stderr, code) = stage.run_paste(
        "\
gh run watch 111 --exit-status
gh run watch 222
echo AFTER
",
    );
    assert_eq!(code, 1, "{stderr}");
    assert!(!stdout.contains("AFTER"), "{stdout}");
    assert_eq!(
        stage.watch_log(),
        "\
run watch 111 --exit-status
run watch 222 --exit-status
"
    );
}

#[test]
fn a_failed_push_stops_the_paste_and_names_the_push() {
    let stage = Stage::new();
    let repo = stage.repo("client", SHA_A, "master");
    fs::write(repo.join(".fake-push-fail"), "").unwrap();
    let script = format!("cd {}\ngit status\ngit push\necho AFTER\n", repo.display());
    let (stdout, stderr, code) = stage.run_paste(&script);
    assert_eq!(code, 1, "{stderr}");
    assert_eq!(stdout, "CLEAN\n");
    assert!(!stdout.contains("AFTER"), "{stdout}");
    assert!(stderr.contains("run: exited 1\ngit push\n"), "{stderr}");
    assert!(!stderr.contains("return"), "{stderr}");
}

#[test]
fn a_watch_uses_the_push_in_the_repository_it_runs_in() {
    let stage = Stage::new();
    let client = stage.repo("client", SHA_A, "master");
    let cms = stage.repo("cms", SHA_B, "test");
    stage.runs(
        SHA_A,
        "body",
        r#"[{"databaseId":999,"createdAt":"2099-01-01T00:00:00Z","headBranch":"v0.2.0"}]"#,
    );
    stage.runs(
        SHA_B,
        "body",
        r#"[{"databaseId":333,"createdAt":"2099-01-01T00:00:00Z","headBranch":"test"}]"#,
    );
    let script = format!(
        "cd {client}\ngit push origin v0.2.0\ncd {cms}\ngit push\ngh run watch\necho AFTER\n",
        client = client.display(),
        cms = cms.display(),
    );
    let (stdout, stderr, code) = stage.run_paste(&script);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("AFTER"), "{stdout}");
    assert_eq!(stage.watch_log(), "run watch 333 --exit-status\n");
}

#[test]
fn a_watch_with_no_push_keeps_gh_behavior() {
    let stage = Stage::new();
    let (stdout, stderr, code) = stage.run_paste("gh run watch\necho AFTER\n");
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("AFTER"), "{stdout}");
    assert_eq!(stage.watch_log(), "run watch --exit-status\n");
    assert!(!stderr.contains("no workflow run appeared"), "{stderr}");
}

#[test]
fn a_watch_stops_when_the_push_never_produces_a_run() {
    let stage = Stage::new();
    let repo = stage.repo("client", SHA_A, "master");
    stage.runs(
        SHA_A,
        "body",
        r#"[{"databaseId":111,"createdAt":"2000-01-01T00:00:00Z","headBranch":"v0.2.0"}]"#,
    );
    let script = format!(
        "cd {}\ngit push origin v0.2.0\ngh run watch\necho AFTER\n",
        repo.display()
    );
    let (stdout, stderr, code) = stage.run_paste(&script);
    assert_eq!(code, 1, "{stderr}");
    assert!(!stdout.contains("AFTER"), "{stdout}");
    assert_eq!(stage.watch_log(), "");
    assert!(
        stderr.contains("no workflow run appeared for v0.2.0"),
        "{stderr}"
    );
    assert!(stderr.contains("run: exited 1"), "{stderr}");
}
