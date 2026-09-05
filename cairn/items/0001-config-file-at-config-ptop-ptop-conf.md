---
id: 1
title: Config file at ~/.config/ptop/ptop.conf
type: feature
status: backlog
milestone: v1.0
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: m
area: config
---

## Problem

Every setting is a command-line flag: `--color`, `--theme`, `--glyphs`. That is
fine for three and unworkable for the dozen this project is heading towards —
sample interval, history length, thresholds, default sort, which columns start
visible. Nobody wants to type them, and a shell alias is not a config system.

## Proposal

Hand-rolled `key = value`, not TOML.

ptop's config surface is genuinely flat, and `serde` + `toml` would be the
largest dependency in the project by an order of magnitude — in a codebase whose
`/proc` parser is deliberately hand-rolled with no dependencies at all. htop and
btop both use key=value and neither has outgrown it. If real nesting ever
appears, that is the moment to reconsider, not before.

    ~/.config/ptop/ptop.conf        # honours XDG_CONFIG_HOME

Precedence, lowest to highest: built-in default, config file, environment
(`NO_COLOR`), command-line flag. The flag always wins, so a wrapper script can
override a user's file.

An unknown key is a warning naming the key and the line, not a hard failure. A
config that refuses to load because of one typo is worse than one that ignores
it and says so.

## Acceptance criteria

- [ ] `key = value` parser with comments, blank lines, and `#` to end of line
- [ ] XDG path, with `$XDG_CONFIG_HOME` respected
- [ ] Every existing flag is settable from the file
- [ ] Flags override the file; the file overrides the built-in defaults
- [ ] Unknown keys warn with key and line number, and do not abort
- [ ] A missing config file is not an error
