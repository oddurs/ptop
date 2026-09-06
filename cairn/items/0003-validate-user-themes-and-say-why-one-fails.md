---
id: 3
title: Validate user themes and say why one fails
type: feature
status: done
milestone: v1.0
created: 2026-09-05
updated: 2026-09-06
priority: p1
effort: s
area: theme
---

## Problem

ptop is the only monitor that measures its own palette. `src/cvd.rs` fails CI if
any pair of meaning-bearing hues drops below ΔE 8 under Machado 2009 simulation,
or if contrast falls below 3:1 against the surface or the selected row. That
guarantee is the entire point of the C series.

The moment users can supply themes, it evaporates — unless the validator is
turned outward.

## Proposal

    ptop --check-theme nord

prints the ΔE matrix and the contrast ratios, and names the pairs that fail:

    nord: FAIL
      series_cpu ↔ ok      ΔE  6.1  (deutan)   below the target of 8
      critical vs selected row     2.4:1       below 3:1
      ...
      worst pair: 6.1   worst contrast: 2.4:1

A failing theme still **loads**, with one warning line. It is the user's
terminal and their choice; ptop's job is to have the number and say it, not to
refuse. That is the same principle as rendering `—` rather than a fabricated
zero.

The side effect worth having: a contributed theme arrives with a measurement
rather than a screenshot.

## Acceptance criteria

- [ ] `--check-theme NAME` reports every pair and both contrast backgrounds
- [ ] Exit status is non-zero on failure, so it is usable in a script
- [ ] Loading a failing theme warns once, naming the worst pair, and continues
- [ ] The built-in themes pass their own check — the same assertion CI already makes
