---
id: 5
title: Documented responsive degradation ladder
type: feature
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: m
area: layout
---

## Problem

Degradation is decided element by element and written down nowhere. The G and L
series each made a yield-order call in isolation — the gutter drops below 30
columns, the heat scale yields before the meters, a section too short to carry
both axis ends carries neither, cores past the width are summarised. Those are
all defensible; together they are not a design, and nothing stops the next one
contradicting them.

## Proposal

Write the order down, then test it:

    scale legend → axis anchors → series labels → gutter → core meters
    → timeline rows → IO columns → tree prefixes

Rendered at a spread of sizes from 200×60 down to 20×8, asserting each element
is present above its threshold and absent below it. bottom does this implicitly
(`should_hide_x_label`, legend skipped under 6 columns); making it explicit is
what stops a layout collapsing surprisingly at some width nobody tested.

## Acceptance criteria

- [ ] A table in `docs/roadmaps/03-layout-and-density.md` listing each element and its threshold
- [ ] One test walking the ladder across sizes, asserting presence and absence
- [ ] The thresholds already shipped are reconciled with the table, or changed to match it
