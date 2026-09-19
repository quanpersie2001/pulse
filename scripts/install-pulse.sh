#!/usr/bin/env bash
set -euo pipefail

REPOSITORY_URL_DEFAULT="https://github.com/quanpersie2001/pulse.git"
REF_DEFAULT="main"

usage() {
  cat <<'EOF'
Usage: install-pulse.sh [options] [target-repository]

Install the Rust `pulse` CLI, initialize a target repository, and install the
Pulse guidance skills for its detected coding-agent hosts.

Options:
  -d, --directory <path>  Target repository. Defaults to the current directory.
  -y, --yes               Skip the confirmation prompt.
      --ref <ref>          Git branch, tag, or commit to install. Default: main.
      --source <path>      Install from a local Pulse checkout instead of GitHub.
      --install-root <dir> Cargo install root. Binary lands at <dir>/bin/pulse.
      --host <host>        Install skills for this host. Repeatable.
      --all-detected       Install skills for every host detected in the target.
      --no-skills          Initialize Pulse without installing guidance skills.
      --with-qa-templates  Copy the qa-ui/qa-api runner templates during init.
      --no-register        Do not register the target for `pulse serve`.
      --binary-only        Install the CLI only; do not initialize a repository.
      --force              Force Cargo to reinstall the selected Pulse version.
      --dry-run            Print the commands without changing anything.
  -h, --help               Show this help.

Remote install (run from the target repository):
  curl -fsSL "https://raw.githubusercontent.com/quanpersie2001/pulse/main/scripts/install-pulse.sh?$(date +%s)" |
    bash -s -- --yes

Select one host (use --ref <release-tag> when a pinned release is desired):
  curl -fsSL https://raw.githubusercontent.com/quanpersie2001/pulse/main/scripts/install-pulse.sh |
    bash -s -- --yes --host claude

Local checkout / isolated install root:
  scripts/install-pulse.sh --source . --directory /path/to/repo --install-root /tmp/pulse-bin --yes

Safety:
  The target must be the root of an existing git worktree. `pulse init` never
  overwrites repository-owned files; an existing installation is refreshed
  through its three-way merge. This script never edits coding-host settings.
  For Claude Code it prints the `pulse hook snippet claude` follow-up command.
EOF
}

log() {
  printf '%s\n' "$*"
}

fail() {
  printf 'Error: %s\n' "$*" >&2
  exit 1
}

can_prompt() {
  [ -r /dev/tty ] && [ -w /dev/tty ]
}

absolute_dir() {
  local path="$1"
  [ -d "$path" ] || fail "Directory does not exist: $path"
  (cd "$path" && pwd -P)
}

print_command() {
  printf '  '
  printf '%q ' "$@"
  printf '\n'
}

run_command() {
  if [ "$DRY_RUN" -eq 1 ]; then
    print_command "$@"
  else
    "$@"
  fi
}

package_version() {
  awk '
    $0 == "[package]" { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^[[:space:]]*version[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=[[:space:]]*/, "", value)
      gsub(/[\"[:space:]]/, "", value)
      print value
      exit
    }
  '
}

ref_install_args() {
  local ref="$1"
  case "$ref" in
    v[0-9]*.[0-9]*.[0-9]*)
      printf '%s\n' "--tag" "$ref"
      ;;
    [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*)
      printf '%s\n' "--rev" "$ref"
      ;;
    *)
      printf '%s\n' "--branch" "$ref"
      ;;
  esac
}

TARGET_INPUT="."
YES=0
REF="${PULSE_INSTALL_REF:-$REF_DEFAULT}"
SOURCE_INPUT="${PULSE_INSTALL_SOURCE:-}"
INSTALL_ROOT="${PULSE_INSTALL_ROOT:-}"
ALL_DETECTED=0
NO_SKILLS=0
WITH_QA_TEMPLATES=0
NO_REGISTER=0
BINARY_ONLY=0
FORCE=0
DRY_RUN=0
HOSTS=()
POSITIONAL_TARGET=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    -d|--directory)
      [ "$#" -ge 2 ] || fail "$1 requires a path"
      TARGET_INPUT="$2"
      shift 2
      ;;
    -y|--yes)
      YES=1
      shift
      ;;
    --ref)
      [ "$#" -ge 2 ] || fail "--ref requires a branch, tag, or commit"
      REF="$2"
      shift 2
      ;;
    --source)
      [ "$#" -ge 2 ] || fail "--source requires a local Pulse checkout"
      SOURCE_INPUT="$2"
      shift 2
      ;;
    --install-root)
      [ "$#" -ge 2 ] || fail "--install-root requires a directory"
      INSTALL_ROOT="$2"
      shift 2
      ;;
    --host)
      [ "$#" -ge 2 ] || fail "--host requires a host name"
      HOSTS+=("$2")
      shift 2
      ;;
    --all-detected)
      ALL_DETECTED=1
      shift
      ;;
    --no-skills)
      NO_SKILLS=1
      shift
      ;;
    --with-qa-templates)
      WITH_QA_TEMPLATES=1
      shift
      ;;
    --no-register)
      NO_REGISTER=1
      shift
      ;;
    --binary-only)
      BINARY_ONLY=1
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      fail "Unknown option: $1"
      ;;
    *)
      [ -z "$POSITIONAL_TARGET" ] || fail "Only one target repository is supported"
      POSITIONAL_TARGET="$1"
      shift
      ;;
  esac
done

if [ "$#" -gt 0 ]; then
  [ -z "$POSITIONAL_TARGET" ] || fail "Only one target repository is supported"
  POSITIONAL_TARGET="$1"
  shift
fi
[ "$#" -eq 0 ] || fail "Unexpected extra arguments"
[ -z "$POSITIONAL_TARGET" ] || TARGET_INPUT="$POSITIONAL_TARGET"

[ "$ALL_DETECTED" -eq 0 ] || [ "${#HOSTS[@]}" -eq 0 ] ||
  fail "Use either --all-detected or --host, not both"
[ "$NO_SKILLS" -eq 0 ] || [ "${#HOSTS[@]}" -eq 0 ] ||
  fail "--no-skills cannot be combined with --host"
[ "$NO_SKILLS" -eq 0 ] || [ "$ALL_DETECTED" -eq 0 ] ||
  fail "--no-skills cannot be combined with --all-detected"
[ "$BINARY_ONLY" -eq 0 ] || [ "$WITH_QA_TEMPLATES" -eq 0 ] ||
  fail "--binary-only cannot be combined with --with-qa-templates"
[ "$BINARY_ONLY" -eq 0 ] || [ "$NO_REGISTER" -eq 0 ] ||
  fail "--binary-only cannot be combined with --no-register"
[ "$BINARY_ONLY" -eq 0 ] || [ "$NO_SKILLS" -eq 0 ] ||
  fail "--binary-only already skips skills; remove --no-skills"
[ "$BINARY_ONLY" -eq 0 ] || [ "${#HOSTS[@]}" -eq 0 ] ||
  fail "--binary-only cannot be combined with --host"
[ "$BINARY_ONLY" -eq 0 ] || [ "$ALL_DETECTED" -eq 0 ] ||
  fail "--binary-only cannot be combined with --all-detected"

command -v cargo >/dev/null 2>&1 || fail "cargo is required (Rust 1.78 or newer)"
command -v git >/dev/null 2>&1 || fail "git is required"

SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" 2>/dev/null && pwd -P || printf '')"
SOURCE_ROOT=""
if [ -n "$SOURCE_INPUT" ]; then
  SOURCE_ROOT="$(absolute_dir "$SOURCE_INPUT")"
elif [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ]; then
  SOURCE_ROOT="$(absolute_dir "$SCRIPT_DIR/..")"
fi

REPOSITORY_URL="${PULSE_INSTALL_REPOSITORY:-$REPOSITORY_URL_DEFAULT}"

if [ -n "$SOURCE_ROOT" ]; then
  [ -f "$SOURCE_ROOT/Cargo.toml" ] || fail "Local Pulse source has no Cargo.toml: $SOURCE_ROOT"
  EXPECTED_VERSION="$(package_version < "$SOURCE_ROOT/Cargo.toml")"
  [ -n "$EXPECTED_VERSION" ] || fail "Could not read package version from $SOURCE_ROOT/Cargo.toml"
  INSTALL_SOURCE="local source $SOURCE_ROOT"
else
  case "$REF" in
    v[0-9]*.[0-9]*.[0-9]*) EXPECTED_VERSION="${REF#v}" ;;
    *) EXPECTED_VERSION="" ;;
  esac
  INSTALL_SOURCE="$REPOSITORY_URL @ $REF"
fi
DISPLAY_VERSION="${EXPECTED_VERSION:-ref $REF}"

TARGET_DIR=""
if [ "$BINARY_ONLY" -eq 0 ]; then
  TARGET_DIR="$(absolute_dir "$TARGET_INPUT")"
  WORKTREE_ROOT="$(git -C "$TARGET_DIR" rev-parse --show-toplevel 2>/dev/null || true)"
  [ -n "$WORKTREE_ROOT" ] || fail "Target is not inside a git worktree: $TARGET_DIR"
  WORKTREE_ROOT="$(absolute_dir "$WORKTREE_ROOT")"
  [ "$WORKTREE_ROOT" = "$TARGET_DIR" ] ||
    fail "Target must be the git worktree root: $WORKTREE_ROOT"
fi

if [ "$YES" -eq 0 ] && [ "$DRY_RUN" -eq 0 ]; then
  can_prompt || fail "Non-interactive install requires --yes"
  if [ "$BINARY_ONLY" -eq 1 ]; then
    printf 'Install Pulse %s from %s? [y/N] ' "$DISPLAY_VERSION" "$INSTALL_SOURCE" > /dev/tty
  else
    printf 'Install Pulse %s and bootstrap %s? [y/N] ' "$DISPLAY_VERSION" "$TARGET_DIR" > /dev/tty
  fi
  IFS= read -r reply < /dev/tty
  case "$reply" in
    y|Y|yes|YES) ;;
    *) fail "Installation cancelled" ;;
  esac
fi

CARGO_ARGS=(install --locked)
# `main` may move without a Cargo version bump between releases; force that
# development channel so rerunning the bootstrap actually updates the binary.
if [ "$FORCE" -eq 1 ] || { [ -z "$SOURCE_ROOT" ] && [ "$REF" = "main" ]; }; then
  CARGO_ARGS+=(--force)
fi
if [ -n "$INSTALL_ROOT" ]; then
  case "$INSTALL_ROOT" in
    /*) ;;
    *) INSTALL_ROOT="$PWD/$INSTALL_ROOT" ;;
  esac
  if [ "$DRY_RUN" -eq 0 ]; then
    mkdir -p "$INSTALL_ROOT"
    INSTALL_ROOT="$(absolute_dir "$INSTALL_ROOT")"
  fi
  CARGO_ARGS+=(--root "$INSTALL_ROOT")
fi
if [ -n "$SOURCE_ROOT" ]; then
  CARGO_ARGS+=(--path "$SOURCE_ROOT")
else
  REF_ARGS=()
  while IFS= read -r item; do
    REF_ARGS+=("$item")
  done < <(ref_install_args "$REF")
  CARGO_ARGS+=(--git "$REPOSITORY_URL" "${REF_ARGS[@]}")
fi

log "Pulse source: $INSTALL_SOURCE"
[ "$DRY_RUN" -eq 0 ] || log "Dry run: no changes will be written."
run_command cargo "${CARGO_ARGS[@]}"

if [ -n "$INSTALL_ROOT" ]; then
  PULSE_BIN="$INSTALL_ROOT/bin/pulse"
else
  CARGO_BIN_ROOT="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
  PULSE_BIN="$CARGO_BIN_ROOT/bin/pulse"
fi

if [ "$DRY_RUN" -eq 0 ]; then
  [ -x "$PULSE_BIN" ] || PULSE_BIN="$(command -v pulse || true)"
  [ -n "$PULSE_BIN" ] && [ -x "$PULSE_BIN" ] ||
    fail "Cargo completed but the pulse binary was not found; add ~/.cargo/bin to PATH"
  REPORTED_VERSION="$("$PULSE_BIN" --version | awk '{ print $2; exit }')"
  [ -n "$REPORTED_VERSION" ] || fail "Installed pulse binary did not report a version"
  if [ -n "$EXPECTED_VERSION" ] && [ "$REPORTED_VERSION" != "$EXPECTED_VERSION" ]; then
    fail "Installed binary identity mismatch: expected $EXPECTED_VERSION, got $REPORTED_VERSION"
  fi
  EXPECTED_VERSION="$REPORTED_VERSION"
fi

if [ "$BINARY_ONLY" -eq 0 ]; then
  INIT_ARGS=("$PULSE_BIN" --repo-root "$TARGET_DIR" init)
  [ -f "$TARGET_DIR/.pulse/issues.jsonl" ] && INIT_ARGS+=(--refresh)
  [ "$WITH_QA_TEMPLATES" -eq 0 ] || INIT_ARGS+=(--with-qa-templates)
  [ "$NO_REGISTER" -eq 0 ] || INIT_ARGS+=(--no-register)
  run_command "${INIT_ARGS[@]}"

  if [ "$NO_SKILLS" -eq 0 ]; then
    SKILL_ARGS=("$PULSE_BIN" --repo-root "$TARGET_DIR" skills install)
    if [ "${#HOSTS[@]}" -gt 0 ]; then
      for host in "${HOSTS[@]}"; do
        SKILL_ARGS+=(--host "$host")
      done
    else
      # A curl pipe has no terminal stdin. Detected hosts are the only safe
      # non-interactive default; an explicit --host remains available.
      SKILL_ARGS+=(--all-detected)
    fi
    run_command "${SKILL_ARGS[@]}"
  fi
fi

if [ "$DRY_RUN" -eq 1 ]; then
  log "Dry run complete."
  exit 0
fi

log ""
log "Pulse $EXPECTED_VERSION installed: $PULSE_BIN"
if [ "$BINARY_ONLY" -eq 0 ]; then
  log "Repository initialized: $TARGET_DIR"
  if [ "$NO_SKILLS" -eq 0 ]; then
    log "Guidance skills installed."
  fi
  log ""
  log "Next:"
  log "  1. Read AGENTS.md, PULSE.md and .pulse/prompts/host.md in the target."
  log "  2. For Claude Code, print and paste the edit hook configuration:"
  print_command "$PULSE_BIN" --repo-root "$TARGET_DIR" hook snippet claude
  log "  3. Open a host-agent session and give it the desired outcome; the agent creates and runs Tickets."
fi
