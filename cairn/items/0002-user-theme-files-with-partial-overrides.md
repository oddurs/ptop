---
id: 2
title: User theme files with partial overrides
type: feature
status: done
milestone: v1.0
created: 2026-09-05
updated: 2026-09-06
priority: p2
effort: m
area: theme
---

## Problem

The palette is compiled in. A user who wants ptop to match their terminal
colours has to fork it.

btop ships 41 themes because the format is one file and one line per colour;
htop has eight, hardcoded in C, and nobody has ever added a ninth. The barrier
decides whether a theme library exists.

## Proposal

One file per theme, `key = value`, in `~/.config/ptop/themes/`:

    ~/.config/ptop/themes/nord.theme

The themeable tokens are exactly the ones `C1` already named — `ok`, `warn`,
`critical`, `series_cpu`, `series_mem`, `chrome`, `text`, `text_dim`,
`selection_bg`, `live`. That vocabulary was built for this; nothing new is
needed.

**Partial overrides, inheriting from `safe`.** Requiring all ten keys is the
reason people do not write themes. Overriding two should be two lines.

Ship `safe.theme` and `classic.theme` as real files as well as built-ins, so the
way to learn the format is to copy one.

## Acceptance criteria

- [ ] `--theme=NAME` resolves a built-in first, then `~/.config/ptop/themes/NAME.theme`
- [ ] A theme may set any subset of tokens; the rest inherit from `safe`
- [ ] Hex (`#5ccfe6`), 256-index (`80`) and ANSI names all parse
- [ ] A malformed value warns with the token and line, and that token falls back
- [ ] The two built-ins are also shipped as files, byte-identical to the compiled ones
