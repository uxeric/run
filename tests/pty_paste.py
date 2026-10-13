#!/usr/bin/env python3
"""Drive the interactive `run` prompt through a pty and print what the terminal showed."""

import os
import pty
import select
import sys
import time

bin_path = sys.argv[1]
mode = sys.argv[2]


def read_for(fd, seconds):
    data = b""
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        data += chunk
    return data


def main():
    pid, fd = pty.fork()
    if pid == 0:
        os.environ.pop("BASH_ENV", None)
        os.environ.pop("SHELLOPTS", None)
        os.environ.pop("BASHOPTS", None)
        os.environ["NO_COLOR"] = "1"
        os.environ["RUN_NO_UPDATE_CHECK"] = "1"
        os.execv(bin_path, [bin_path])

    shown = b""
    deadline = time.time() + 3
    while time.time() < deadline and b"Ctrl-D" not in shown:
        shown += read_for(fd, 0.1)
    if b"Ctrl-D" not in shown:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
        sys.stderr.buffer.write(shown)
        sys.stderr.write("\nrun did not reach the paste prompt\n")
        return 2

    if mode == "paste":
        payload = (
            b"\x1b[200~printf 'OUT_A\\n' # printf 'NO_RUN\\n'\r\n"
            b"printf 'OUT_B\\n'\r\n"
            b"\x1b[201~"
        )
    elif mode == "ctrld":
        payload = b"printf 'OUT_C\\n'\r\n\x04"
    elif mode == "backspace":
        payload = b"printf 'Z\x7fOUT_D\\n'\r\n\x04"
    else:
        sys.stderr.write(f"unknown mode {mode}\n")
        os.kill(pid, 9)
        os.waitpid(pid, 0)
        return 2

    os.write(fd, payload)
    code = 1
    deadline = time.time() + 3
    while time.time() < deadline:
        shown += read_for(fd, 0.1)
        wpid, status = os.waitpid(pid, os.WNOHANG)
        if wpid == 0:
            continue
        shown += read_for(fd, 0.2)
        if os.WIFEXITED(status):
            code = os.WEXITSTATUS(status)
        elif os.WIFSIGNALED(status):
            code = 128 + os.WTERMSIG(status)
        break
    else:
        os.kill(pid, 9)
        os.waitpid(pid, 0)
        sys.stderr.buffer.write(shown)
        sys.stderr.write("\nrun did not finish\n")
        return 2

    sys.stdout.buffer.write(shown)
    return code


if __name__ == "__main__":
    sys.exit(main())
