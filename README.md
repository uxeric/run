<div align="center">

<img src="assets/readme/hero.svg" width="100%" alt="run waits on a new line. A paste appears with its line breaks and a comment, then the commands run and the comment is not a command.">

# run

**Paste the commands. They run in order.**<br>
One shell for the whole paste. Comments stay comments.

<a href="#jack-in"><img alt="built with Rust" src="https://img.shields.io/badge/built_with-Rust-ff2a6d?style=for-the-badge&logo=rust&logoColor=white&labelColor=07060f"></a>
<a href="#the-paste"><img alt="paste then run" src="https://img.shields.io/badge/paste-then_run-29f0ff?style=for-the-badge&labelColor=07060f"></a>
<a href="https://omarchy.org"><img alt="made for Omarchy 4" src="https://img.shields.io/badge/made_for-Omarchy_4-d6dcf5?style=for-the-badge&labelColor=07060f"></a>

<kbd>[jack in](#jack-in)</kbd>&nbsp;
<kbd>[the paste](#the-paste)</kbd>&nbsp;
<kbd>[faq](#faq)</kbd>

</div>

`run` waits on a new line, shows the paste with its line breaks, and runs each command. `#` comments are ignored. Quotes and heredocs keep their text.

<img src="assets/readme/divider.svg" width="100%" alt="">

## Jack in

```sh
curl -fsSL https://raw.githubusercontent.com/uxeric/run/HEAD/install.sh | bash
```

<img src="assets/readme/divider.svg" width="100%" alt="">

## The paste

```sh
run
```

Paste ends, and this runs. Ctrl-D runs what you typed.

```text
cd ~/app
git status -sb                # expect "## main", nothing behind
git commit -F- <<'EOF'
Adds the paste runner
EOF
```

The commands share one shell, so `cd` sticks. The first command that exits non-zero stops the run. Close a heredoc. If its closing line is missing, nothing runs.

A newer version asks before it replaces this one. Enter installs it and continues. `n` keeps this version. `run --update` installs without asking.

<img src="assets/readme/divider.svg" width="100%" alt="">

## FAQ

> [!NOTE]
> **Does the comment run?**<br>
> No. `#` is ignored, including after a command. Text inside quotes and heredocs is kept.

> [!IMPORTANT]
> **What if the heredoc never closes?**<br>
> Nothing runs. Add the closing line and paste again.

<div align="center">
<br>

<img src="assets/readme/divider.svg" width="60%" alt="">

<sub><code>// the comment is not a command</code></sub>

</div>
