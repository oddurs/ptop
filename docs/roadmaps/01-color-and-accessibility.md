# Colour and accessibility

ptop currently calls `Color::Green`, `Color::Yellow`, `Color::Red` directly from
`ui.rs`. Three consequences, in increasing order of severity: the palette cannot
be changed, ptop does not actually control the hues, and the pair it depends on
most is invisible to the most common form of colour blindness.

## The measurement

Run against a dark surface (`#1a1a19`), adjacent-pair separation in OKLab ΔE×100
(target ≥ 8 under simulated colour-vision deficiency, ≥ 15 for normal vision):

| Palette | protan ΔE | normal ΔE | contrast |
|---|---|---|---|
| ptop today — ANSI green/yellow/red | **3.7** | 16.3 | red 2.98:1 (below 3:1) |
| btop's muted heat `#77ca9b,#cbc06c,#dc4c4c` | 5.8 | **10.3** | pass |
| `#5ccfe6,#ffd580,#ff6666` | **16.2** | 22.1 | pass |
| `#73d0ff,#ffcc66,#f28779` | 14.7 | 18.1 | pass |

ptop's green↔yellow pair is **ΔE 3.7** under protanopia — effectively one colour
for roughly 8% of men. btop's muted variant is worse still: at ΔE 10.3 it is
hard to separate even with full colour vision.

Lightness also runs 0.735 → 0.821 → 0.533. Yellow is *brighter* than green and
red is darker than both, so in greyscale or under low vision "hotter" does not
read monotonically: 50% CPU looks more prominent than 100%.

---

## C1 — Theme tokens  ·  `S`  ·  **DONE**

**What.** A `Theme` struct of named semantic tokens — `ok`, `warn`, `critical`,
`chrome`, `label_dim`, `cursor`, `series_cpu`, `series_mem` — threaded through
`ui.rs`. No `Color::` literal survives outside `theme.rs`.

**Why.** Unblocks every other item here. Also enforces a rule ptop currently
breaks in passing: text should wear text tokens, never a series colour.

**Acceptance.** `grep -rn 'Color::' src/ui.rs` returns nothing. Rendering is
byte-identical to today under the default theme (assert with an existing
`TestBackend` snapshot).

**Touches.** `src/theme.rs` (new), `src/ui.rs`

---

## C2 — Colour tiers with capability detection  ·  `M`

**What.** Three tiers, each independently complete: **monochrome** (usable),
**ANSI-16** (readable), **256/truecolor** (intended). Detect via `COLORTERM`
and `TERM`; override with `--color=auto|mono|16|256|true`.

**Why.** `Color::Green` is an ANSI-16 slot whose actual hue is chosen by *the
user's terminal theme*, not by ptop — so none of the measurements above are
guaranteed to hold in practice. Only the 256/truecolor tier gives real control.
Monochrome is not a courtesy tier: it is the proof that no meaning is carried by
colour alone, which is the requirement `C3` cannot satisfy on its own.

**Acceptance.** Every threshold and series remains identifiable at the
monochrome tier. Snapshot test per tier. `--color=mono` output contains no SGR
colour sequences.

**Depends on.** `C1`

**Touches.** `src/theme.rs`, `src/main.rs`

---

## C3 — Colour-vision-safe default palette  ·  `S`

**What.** Default to cyan/amber/red (`#5ccfe6,#ffd580,#ff6666` measured above)
at the 256/truecolor tier. Keep green/yellow/red available as
`--theme=classic`.

**Why.** Takes the worst adjacent pair from ΔE 3.7 to 16.2. Every system monitor
ships green/yellow/red; this is a cheap, measurable way to be better than all of
them.

**The tradeoff, stated.** Green-means-good is a strong convention and breaking
it has a real cost for the majority who can see it. That is why `classic` stays
one flag away — and why `C2`'s monochrome tier matters more than the hue choice:
if the display is only legible in colour, no palette fixes it.

**Acceptance.** Default passes CVD separation, normal-vision floor and contrast.
`--theme=classic` reproduces today's colours.

**Depends on.** `C1`, `C2`

**Touches.** `src/theme.rs`

---

## C4 — Palette validation in CI  ·  `S`

**What.** A Rust test computing OKLab ΔE between adjacent palette slots under
normal, protan, deutan and tritan simulation. Fails the build below threshold.

**Why.** A palette regresses the moment someone tweaks a hex by eye. The check
is arithmetic, so it belongs in CI rather than in review.

**Acceptance.** Test fails when a slot is deliberately moved to a near-neighbour.
Every shipped theme passes. Runs in existing CI with no new dependency.

**Depends on.** `C1`

**Touches.** `src/theme.rs`, `.github/workflows/ci.yml`

---

## C5 — Recessive chrome  ·  `S`

**What.** Borders, gridlines, axis labels and inactive text move to a token one
step off the surface, not default foreground.

**Why.** Chrome currently renders at the same weight as data. Gridlines and
axes should be solid hairlines one shade off the surface so the data is what
carries visual weight.

**Acceptance.** Chrome contrast against surface sits below data contrast, and
above the legibility floor. Visual check via `show_frame`.

**Depends on.** `C1`

**Touches.** `src/theme.rs`, `src/ui.rs`

---

## C6 — Status colours reserved  ·  `S`

**What.** Audit every use of `ok`/`warn`/`critical` and confirm each means a
*state*, not an identity. Series identity uses series tokens only.

**Why.** Status tokens reused for "series 3" destroy the meaning of the status
colour everywhere else. `G5` introduces series colours to the timeline, which is
exactly when this goes wrong.

**Acceptance.** Documented in `theme.rs`: which tokens are status, which are
identity, and that the sets never overlap.

**Depends on.** `C1`, `G5`

**Touches.** `src/theme.rs`
