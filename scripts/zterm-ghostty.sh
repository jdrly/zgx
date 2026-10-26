#!/bin/sh
# `ghostty +ssh` for zTerm, which has no Ghostty CLI beside it: Ghostty's
# shell integration wraps `ssh` with it when `ssh-env` is on (zTerm turns it
# on, terminal/session.rs). Hosts rarely know xterm-ghostty, so ssh asks for
# xterm-256color there and passes COLORTERM and TERM_PROGRAM on, as Ghostty's
# own does (src/cli/ssh.zig). Its `ssh-terminfo` (installing the terminfo on
# the host) is not here: those hosts get xterm-256color too.
if [ "${1-}" != +ssh ]; then
  echo "zTerm: only 'ghostty +ssh' is available here" >&2
  exit 64
fi
shift
forward=true
while [ $# -gt 0 ]; do
  case $1 in
    --) shift; break ;;
    --forward-env=false) forward=false ;;
    --forward-env | --forward-env=true) ;;
    --terminfo* | --cache* | --verbose*) ;;
    *) break ;;
  esac
  shift
done
if [ "$forward" = true ]; then
  exec ssh -o SetEnv=TERM=xterm-256color -o SendEnv=COLORTERM \
    -o SendEnv=TERM_PROGRAM -o SendEnv=TERM_PROGRAM_VERSION "$@"
fi
TERM=xterm-256color exec ssh "$@"
