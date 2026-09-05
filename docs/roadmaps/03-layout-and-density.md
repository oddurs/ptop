# Layout and density

Four bordered panels on a 30-row terminal spend **8 rows on box-drawing alone** —
27% of the vertical space, before any data. The cores panel is the clearest
case: three rows of allocation for one row of glyphs, so two thirds of it is
chrome.

Current layout (`src/ui.rs`):

```
Length(3)   header      1 data row + 2 border
Length(3)   cores       1 data row + 2 border
Length(10)  timeline    8 data rows + 2 border
Min(5)      processes   n data rows + 2 border
Length(1)   help
```

The timeline is the reason the tool exists and gets 8 usable rows; the panels
around it consume 6 rows to draw boxes.

---

## L1 — Replace panel borders with dividers  ·  `M`  ·  **DONE**

**What.** Drop `Borders::ALL` in favour of a single dim horizontal rule between
sections, with the section name inline on that rule.

**Why.** Recovers 5 rows and 2 columns. (The original estimate of "roughly 8
rows" counted the 8 border rows removed but not the 3 divider rows added; the
side borders were not counted at all, and those are the 2 columns.) Heavy chrome also competes
with the data for attention; the fix is hairline separators and breathing room.

**Acceptance.** No visual regression in the panel-fills-its-space test. At least
6 rows recovered at 30 rows tall. Section boundaries still legible in
monochrome.

**Touches.** `src/ui.rs` (`draw`, all `bordered()` call sites)

---

## L2 — Fold the core meters into the header  ·  `S`  ·  **DONE**

**What.** Move the per-core glyph row onto the header line; delete the cores
panel.

**Why.** It is one line of data in a three-line panel. It reads as a status
strip, not a chart, and belongs with the other status figures.

**Acceptance.** Cores remain visible at ≥ 80 columns. Wraps or truncates
gracefully on a many-core machine (test at 128 cores). Header stays one line.

**Depends on.** `L1`

**Touches.** `src/ui.rs`

---

## L3 — Give reclaimed rows to the timeline  ·  `S`  ·  **DONE**

**What.** Timeline height becomes proportional rather than `Length(10)`, taking
the space `L1` and `L2` free up, with a floor and a ceiling.

**Why.** More rows is directly more vertical resolution: each braille row is 4
levels, so 5 rows is 20 distinct heights against today's 12. The existing exact
row-split logic already divides available height between CPU and MEM without
leaving blanks.

**Acceptance.** Timeline grows on tall terminals and never starves the process
table below a usable minimum. Existing small-terminal tests still pass.

**Corrected during implementation.** "Proportional" alone is a regression. A
plain proportion gave an 80×24 terminal five rows where nine were fixed before
— a quarter of the CPU resolution, on the commonest terminal size, from a change
justified by more resolution. Growing a panel must never shrink it, so the old
fixed height is the floor wherever it fits and the proportion only applies above
it.

**Depends on.** `L1`, `L2`

**Touches.** `src/ui.rs`

---

## L4 — Documented responsive ladder  ·  `M`

**What.** Write down, and implement, the order in which elements drop as the
terminal shrinks: scale legend → y anchors → core meters → timeline rows →
IO columns → tree prefixes.

**Why.** Degradation is currently implicit and inconsistent — some elements have
thresholds, others do not. bottom makes this explicit
(`should_hide_x_label`, legend skipped below 6 columns). Deciding the order once
prevents a layout that collapses in a surprising way at some untested width.

**Acceptance.** A table in this file listing each element and the size at which
it drops. A test asserting the ladder holds at a spread of sizes from 200×60
down to 20×8.

**Depends on.** `L1`, `L2`, `L3`

**Touches.** `src/ui.rs`, `src/ui_tests.rs`
