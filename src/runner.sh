# The pasted script is sourced so cd and variables stick, and so bash
# reports errors against the paste file rather than this wrapper.
# The parent clears BASH_ENV, SHELLOPTS, and BASHOPTS before starting bash.
#
# git and gh are functions so two command classes wait the way the rest of
# a paste does. The paste text is still bash; nothing is rewritten.
#   git push      Remember the repo, the commit, and the ref that was pushed.
#   gh run watch  With no run id, watch the run that push started, polling
#                 until GitHub has created it. --exit-status is added when
#                 the command does not already have it, so a failed run
#                 stops the paste. A watch that names a run id keeps that id.
set -e
set -o pipefail
set -E
trap '
ps=("${PIPESTATUS[@]}")
cmd=${__run_fail_cmd:-$BASH_COMMAND}
unset __run_fail_cmd
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

declare -A __run_push_sha=()
declare -A __run_push_at=()
declare -A __run_push_ref=()

__run_forward() {
  local status=0
  __run_fail_cmd=$(printf '%q' "$1")
  if (( $# > 1 )); then
    __run_fail_cmd+=$(printf ' %q' "${@:2}")
  fi
  command "$@" || status=$?
  if (( status == 0 )); then
    unset __run_fail_cmd
    return 0
  fi
  return "$status"
}

__run_current_branch() {
  local name=""
  name=$(command git rev-parse --abbrev-ref HEAD 2>/dev/null) || name=""
  if [[ -z $name || $name == HEAD ]]; then
    return 0
  fi
  printf '%s\n' "$name"
}

__run_is_remote() {
  local name=$1 remote
  case $name in
    *://*|git@*|//*|/*|./*|../*) return 0 ;;
  esac
  while IFS= read -r remote; do
    [[ $remote == "$name" ]] && return 0
  done < <(command git remote 2>/dev/null || true)
  return 1
}

__run_refspec_dest() {
  local spec=${1#+} dest
  dest=${spec##*:}
  dest=${dest#refs/tags/}
  dest=${dest#refs/heads/}
  if [[ -z $dest || $dest == HEAD ]]; then
    __run_current_branch
    return 0
  fi
  printf '%s\n' "$dest"
}

# Sets __run_push_dry and __run_push_target. A dry run is not remembered.
# An empty target means any workflow run on the pushed commit.
__run_push_classify() {
  __run_push_dry=0
  __run_push_target=""
  local -a pos=()
  local arg broad=0
  shift
  while (( $# )); do
    arg=$1
    shift
    case $arg in
      --)
        pos+=("$@")
        break
        ;;
      -n|--dry-run) __run_push_dry=1 ;;
      --all|--mirror|--tags|--follow-tags) broad=1 ;;
      -u|--set-upstream|-f|--force|--force-if-includes|--quiet|-q|--verbose|-v|--progress|--no-progress|--atomic|--prune|--signed|--no-signed|--verify|--no-verify|--thin|--no-thin|--delete|-d|-4|-6) ;;
      --force-with-lease|--force-with-lease=*|--recurse-submodules|--recurse-submodules=*|--signed=*) ;;
      -o|--push-option|--receive-pack|--exec|--repo)
        if (( $# )); then shift; fi
        ;;
      --push-option=*|--receive-pack=*|--exec=*|--repo=*) ;;
      --*) ;;
      -*) [[ $arg == *n* ]] && __run_push_dry=1 ;;
      *) pos+=("$arg") ;;
    esac
  done
  if (( __run_push_dry || broad )); then
    return 0
  fi
  local -a specs=()
  if (( ${#pos[@]} > 0 )) && __run_is_remote "${pos[0]}"; then
    specs=("${pos[@]:1}")
  else
    specs=("${pos[@]}")
  fi
  if (( ${#specs[@]} == 1 )); then
    __run_push_target=$(__run_refspec_dest "${specs[0]}")
  elif (( ${#specs[@]} == 0 )); then
    __run_push_target=$(__run_current_branch)
  fi
  return 0
}

__run_remember_push() {
  local since=$1 top="" sha=""
  top=$(command git rev-parse --show-toplevel 2>/dev/null) || top=""
  sha=$(command git rev-parse HEAD 2>/dev/null) || sha=""
  if [[ -n $top && -n $sha ]]; then
    __run_push_sha[$top]=$sha
    __run_push_at[$top]=$since
    __run_push_ref[$top]=${__run_push_target-}
  fi
  return 0
}

git() {
  if [[ ${1-} != push ]]; then
    __run_forward git "$@"
    return
  fi
  local since=""
  if ! since=$(date -u -d '1 second ago' +%Y-%m-%dT%H:%M:%SZ); then
    since=""
  fi
  __run_push_classify "$@"
  __run_forward git "$@"
  if (( __run_push_dry == 0 )); then
    __run_remember_push "$since"
  fi
}

__run_pick_run() {
  RUN_SINCE=$1 RUN_REF=$2 python3 -c '
import json, os, sys
since = os.environ.get("RUN_SINCE", "")[:19]
ref = os.environ.get("RUN_REF", "")
try:
    runs = json.load(sys.stdin)
except json.JSONDecodeError:
    sys.exit(0)
if not isinstance(runs, list):
    sys.exit(0)
for run in runs:
    created = str(run.get("createdAt") or "")[:19]
    branch = str(run.get("headBranch") or "")
    if since and created < since:
        continue
    if ref and branch != ref:
        continue
    ident = run.get("databaseId")
    if ident is None:
        continue
    print(ident)
    break
'
}

# Sets __run_watch_state to ready, none, timeout, or error.
# ready stores the id in __run_watch_id.
__run_resolve_watch_id() {
  local -a repo_args=("$@")
  __run_watch_id=""
  __run_watch_state=none
  local top="" sha="" since="" ref=""
  top=$(command git rev-parse --show-toplevel 2>/dev/null) || top=""
  if [[ -n $top ]]; then
    sha=${__run_push_sha[$top]-}
    since=${__run_push_at[$top]-}
    ref=${__run_push_ref[$top]-}
  fi
  if [[ -z $sha ]]; then
    return 0
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    printf 'run: python3 is required to match the workflow run\n' >&2
    __run_watch_state=error
    return 0
  fi
  local tries=${RUN_WATCH_TRIES:-20}
  local interval=${RUN_WATCH_INTERVAL:-3}
  [[ $tries =~ ^[0-9]+$ ]] || tries=20
  [[ $interval =~ ^[0-9]+$ ]] || interval=3
  local attempt list="" id=""
  for (( attempt = 1; attempt <= tries; attempt++ )); do
    if ! list=$(command gh run list "${repo_args[@]}" --commit "$sha" --limit 20 --json databaseId,createdAt,headBranch); then
      __run_watch_state=error
      return 0
    fi
    if ! id=$(printf '%s' "$list" | RUN_SINCE="$since" RUN_REF="$ref" __run_pick_run "$since" "$ref"); then
      __run_watch_state=error
      return 0
    fi
    if [[ -n $id ]]; then
      __run_watch_id=$id
      __run_watch_state=ready
      return 0
    fi
    if (( attempt < tries )); then
      sleep "$interval"
    fi
  done
  local what=$ref
  [[ -z $what ]] && what=${sha:0:7}
  printf 'run: no workflow run appeared for %s after the push\n' "$what" >&2
  __run_watch_state=timeout
  return 0
}

gh_run_watch() {
  local -a flags=() repo_args=()
  local run_id="" has_exit=0
  while (( $# )); do
    case $1 in
      --exit-status)
        has_exit=1
        flags+=("$1")
        shift
        ;;
      -i|--interval)
        flags+=("$1")
        shift
        if (( $# )); then
          flags+=("$1")
          shift
        fi
        ;;
      -R|--repo)
        flags+=("$1")
        repo_args+=("$1")
        shift
        if (( $# )); then
          flags+=("$1")
          repo_args+=("$1")
          shift
        fi
        ;;
      --interval=*|--repo=*)
        flags+=("$1")
        [[ $1 == --repo=* ]] && repo_args+=("$1")
        shift
        ;;
      --)
        shift
        if [[ -z $run_id && $# -gt 0 ]]; then
          run_id=$1
        fi
        break
        ;;
      -*)
        flags+=("$1")
        shift
        ;;
      *)
        if [[ -z $run_id ]]; then
          run_id=$1
        fi
        shift
        ;;
    esac
  done
  if (( has_exit == 0 )); then
    flags+=(--exit-status)
  fi
  if [[ -z $run_id ]]; then
    __run_resolve_watch_id "${repo_args[@]}"
    case $__run_watch_state in
      ready) run_id=$__run_watch_id ;;
      timeout|error) return 1 ;;
    esac
  fi
  if [[ -n $run_id ]]; then
    __run_forward gh run watch "$run_id" "${flags[@]}"
  else
    __run_forward gh run watch "${flags[@]}"
  fi
}

gh() {
  if [[ ${1-} == run && ${2-} == watch ]]; then
    __run_fail_cmd=$(printf '%q' gh)
    if (( $# > 0 )); then
      __run_fail_cmd+=$(printf ' %q' "$@")
    fi
    shift 2
    gh_run_watch "$@"
    return
  fi
  __run_forward gh "$@"
}

source "$1"
