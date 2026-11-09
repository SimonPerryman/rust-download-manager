#!/usr/bin/env bash
# Which CI areas a pull request touches, printed as `rust=`, `gui=` and `ci=`
# lines for $GITHUB_OUTPUT.
#
# Usage: changed-areas.sh <event name> [<base sha> <head sha>]
#
# Anything but a clean pull-request diff runs everything: pushes, manual
# dispatch, a missing base or failed diff, a change under .github/, and any
# path not classified below. Docs-only paths set nothing.
set -uo pipefail

emit() { printf 'rust=%s\ngui=%s\nci=%s\n' "$1" "$2" "$3"; }
everything() {
  echo "Running everything: $1" >&2
  emit true true true
  exit 0
}

[ "$1" = pull_request ] || everything "event is $1, not pull_request"
[ -n "${2:-}" ] && [ -n "${3:-}" ] || everything "PR base or head SHA missing"
# --no-renames lists a moved file's old and new paths, so both are classified.
files=$(git diff --name-only --no-renames "$2...$3") || everything "git diff failed"

rust=false
gui=false
while IFS= read -r f; do
  case "$f" in
    "") ;;
    .github/*) everything "CI config changed ($f)" ;;
    # The GUI crate is a Rust workspace member and the GUI embeds the engine.
    gui/src-tauri/*) rust=true gui=true ;;
    gui/*) gui=true ;;
    src/* | tests/* | benches/* | examples/* | .config/* | .cargo/* | \
      Cargo.toml | */Cargo.toml | Cargo.lock | rust-toolchain.toml | \
      deny.toml | audit.toml)
      rust=true
      ;;
    *.md | docs/* | milestones/* | LICENSE*) ;;
    *)
      echo "Unclassified path, running everything: $f" >&2
      rust=true gui=true
      ;;
  esac
done <<<"$files"

emit "$rust" "$gui" false
