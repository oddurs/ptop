---
id: 18
title: Piping --once into head panics
type: bug
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: s
area: cli
---

## Problem

    ptop --once | head -5

panics with `failed printing to stdout: Broken pipe`. Rust ignores SIGPIPE by
default and turns the resulting write error into a panic, so any plain-text
output piped into a command that stops reading early takes the tool down.

Found while piping `--bench` into `head` during the sparkline work.

## Why it matters

`--once` exists precisely to be scriptable, and `| head`, `| grep -m1` and
`| less` are exactly how a scriptable thing gets used. A monitor that panics
when you page its output is not scriptable.

## Proposal

Restore the default `SIGPIPE` disposition at start-up, so the process exits
quietly the way every other Unix tool does. It is a couple of lines and needs
`libc` — or catch the write error and exit 0 without it, which keeps the
dependency list where it is.

## Acceptance criteria

- [ ] `ptop --once | head -1` exits quietly, no panic, no backtrace
- [ ] `ptop --bench | head -1` likewise
- [ ] Full output is unchanged when nothing closes the pipe
