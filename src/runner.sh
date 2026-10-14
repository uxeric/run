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
# Hot #ff2a6d, the same mark as the prompt. Piped stderr stays plain.
if [[ -t 2 && -z ${NO_COLOR:-} ]]; then
  printf "\033[38;2;255;42;109mrun: exited %s\033[0m\n%s\n" "$status" "$cmd" >&2
else
  printf "run: exited %s\n%s\n" "$status" "$cmd" >&2
fi
exit "$status"
' ERR
source "$1"
