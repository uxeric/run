# The pasted script is sourced so cd and variables stick, and so bash
# reports errors against the paste file rather than this wrapper.
# The parent clears BASH_ENV, SHELLOPTS, and BASHOPTS before starting bash.
set -e
set -o pipefail
set -E
trap '
ps=("${PIPESTATUS[@]}") cmd=$BASH_COMMAND
status=0
for s in "${ps[@]}"; do
  if [[ $s -ne 0 ]]; then
    status=$s
  fi
done
cmd=${cmd%"${cmd##*[![:space:]]}"}
printf "run: exited %s\n%s\n" "$status" "$cmd" >&2
exit "$status"
' ERR
source "$1"
