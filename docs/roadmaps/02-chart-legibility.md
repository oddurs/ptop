# Chart legibility

The timeline shows shape and nothing else. There is no axis, no gridline and no
scale marker anywhere in it. A value is readable at the cursor and at no other
point, so a bar at 45% and a bar at 65% are indistinguishable — and the 50%/80%
thresholds that drive every colour decision are themselves invisible.

For comparison, `bottom` draws real y-axis labels on its time graphs and hides
them responsively when the panel gets too small (`should_hide_x_label`,
`src/canvas/widgets/*_graph.rs`).

---

## G1 — Threshold hairline  ·  `S`  ·  **DONE**

**What.** A dim horizontal rule across the timeline at the warn threshold (and
optionally critical), drawn in the chrome token.

**Why.** The single highest-leverage change in this file, because it fixes two
problems at once. The thresholds currently exist *only* as a colour change,
which fails the rule that meaning must never be colour-alone — and which
`01-color-and-accessibility.md` shows is invisible to ~8% of men. A rule makes
the boundary readable without colour, and simultaneously gives the graph the
scale anchor it completely lacks: any bar can now be read as above or below a
known line.

**Acceptance.** Threshold state is determinable from a monochrome screenshot.
Rule renders in the chrome token, never over-printing a data cell in a way that
hides it. Present at every zoom level.

**Touches.** `src/ui.rs` (`draw_timeline`)

---

## G2 — Y-axis anchors  ·  `S`  ·  **DONE**

**What.** `100` and `0` in a narrow left gutter of the timeline, dropped when
the panel is too short or narrow.

**Why.** Completes the scale `G1` starts. Cheap, and matches what bottom does.

**Acceptance.** Gutter never overlaps the graph. Degrades cleanly on a narrow
terminal — covered by the existing `survives_absurdly_small_terminal` test.

**Depends on.** `G1`

**Touches.** `src/ui.rs`

---

## G3 — Direct row labels  ·  `S`  ·  **DONE**

**What.** `CPU` and `MEM` in the timeline's left gutter. Delete the
`cpu (top) · mem (bottom)` legend line.

**Why.** Two series told apart by vertical position plus a legend, when a direct
label is available and unambiguous. Direct labels beat legends. Also reclaims a
row.

**Acceptance.** No legend line. Each graph band is labelled adjacent to itself.

**Depends on.** `G2` (shares the gutter)

**Touches.** `src/ui.rs`

---

## G4 — Value readout at the cursor  ·  `S`  ·  **DONE**

**What.** When scrubbed, print the value at the cursor column — near the cursor,
not only in the header.

**Why.** A TUI has no hover, so the scrub cursor *is* the crosshair. Its readout
currently lives in the header, far from where the eye is fixed. The cursor
already marks which half-cell it is on (`GlyphSet::cursor_marker`); the number
should be beside it.

**Acceptance.** Readout tracks the cursor, never overflows the panel, and
suppresses itself when live. Shows both CPU and MEM at that instant.

**Touches.** `src/ui.rs`

---

## G5 — Colour the timeline by series, not magnitude  ·  `M`  ·  **DONE**

**What.** CPU and MEM each take a categorical series token. Remove the
magnitude-driven `heat()` call from `draw_timeline`.

**Why.** Bar height already encodes the value; colouring by the same value is
double-encoding, which burns the only free channel on information the chart
already shows. Meanwhile the two series are distinguished only by position. Move
colour to the job height cannot do — identity.

**Keep `heat()` where it earns its place:** the header figures and the per-core
meters, where the reader's question is "is this bad right now?" rather than
"what shape was this?". That is status encoding, not a redundant ramp.

**Acceptance.** Timeline colour depends only on which series a row is. Header
and core meters still use status colour. `G1`'s rule still communicates
threshold state.

**Depends on.** `C1`, `G1`

**Touches.** `src/ui.rs`

---

## G7 — The window should follow the cursor  ·  `M`

**What.** When the scrub cursor moves to a sample outside the drawn window,
scroll the window to include it instead of pinning the marker to the left edge.

**Why.** Found while reviewing `G4`. The graph always draws the newest
`slots × zoom` samples regardless of where the cursor is, so pressing `Home` on
a buffer longer than the panel is wide scrubs to a sample that is not on screen.
The process table updates to that moment, the header reports its lag — and the
graph shows a different stretch of time entirely.

`G4` made this honest rather than fixing it: the marker becomes `◀` and the
readout is suppressed, because printing figures for an off-screen sample beside
the column it appears to point at is worse than printing nothing. But scrubbing
to somewhere you cannot see is still a poor answer for a tool whose whole
premise is looking backwards.

**The reason it was not simply fixed.** `History::peak_slots` is right-aligned
by construction, and deliberately so — the newest sample is pinned to the right
edge so slot boundaries do not shuffle sideways once per second. Letting the
window start somewhere other than the live edge means that invariant needs a
second, explicit anchor rather than an implicit one. That is a design change,
not a patch.

**Acceptance.** Scrubbing past the left edge scrolls the window. The newest
sample still pins right whenever the cursor is inside the live window, so the
live view does not shuffle. Zoom continues to work at any scroll position.
`◀` remains for the case where the cursor is genuinely outside the buffer.

**Touches.** `src/history.rs`, `src/ui.rs`

---

## G6 — Scale legend for retained heat  ·  `S`  ·  **DONE**

**What.** Wherever a heat ramp survives `G5`, show its thresholds once —
e.g. `50 / 80` beside the core meters.

**Why.** Semantic heat is an acceptable multi-hue ramp, but only with a scale
legend. Right now the numbers behind the colour change are documented nowhere in
the UI.

**Acceptance.** Every retained heat ramp has a visible scale. Hidden first when
space is tight.

**Depends on.** `G5`

**Touches.** `src/ui.rs`
