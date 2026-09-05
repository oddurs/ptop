# Roadmaps — design rationale

> **Live tracking moved to [cairn](../../cairn/items/).** These files are the
> reasoning: the measurements, the prior-art comparisons, and the arguments that
> produced each decision. They are worth keeping and worth reading, and they are
> no longer where you look to see what is open.
>
> `cairn board` for the current state, `cairn next` for what is ready, or
> [ROADMAP.md](../../ROADMAP.md) rendered from the same files.
>
> Items marked **DONE** below are shipped. Everything still open was moved into
> cairn rather than duplicated — two lists of open work is worse than one.

# Roadmaps

Granular, individually shippable items derived from reading the prior art
(htop, btop, bottom, zenith, atop, sampler, gping, nvtop) and auditing ptop's
UI against data-visualisation practice.

Each item states its evidence, so a future reader can judge whether the
reasoning still holds rather than taking it on faith. Where a claim is
measurable it carries the measurement.

| File | Theme | Items |
|---|---|---|
| [00-positioning.md](00-positioning.md) | Honest claims about what ptop is | X1–X2 |
| [01-color-and-accessibility.md](01-color-and-accessibility.md) | Palette, theming, colour-vision safety | C1–C6 |
| [02-chart-legibility.md](02-chart-legibility.md) | Scale, thresholds, labels, cursor readout | G1–G7 |
| [03-layout-and-density.md](03-layout-and-density.md) | Reclaiming vertical space | L1–L4 |
| [04-process-table.md](04-process-table.md) | Scanning the table, per-process history | P1–P3 |
| [05-data-fidelity.md](05-data-fidelity.md) | The gaps that limit what ptop can answer | D1–D4 |
| [06-collection-efficiency.md](06-collection-efficiency.md) | Making the collector cheap enough for faster sampling | E1–E2 |

## Suggested order

**Do first — cheap and high-leverage**

1. `X1` correct the README's claims (they are currently wrong)
2. `G1` threshold hairline — fixes the missing scale *and* the colour-only
   encoding in one change
3. `C1` theme tokens — unblocks every other colour item

**Then — the visible quality jump**

4. `L1` drop panel borders (reclaims ~27% of vertical space)
5. `C2`+`C3` colour tiers and a colour-vision-safe default
6. `G3`+`G4` direct labels and a readout at the cursor

**Then — the differentiator**

7. `P3` per-process sparklines. Nothing else can do this, because nothing else
   retains per-process history. See `04-process-table.md`.
8. `E1`+`E2` collection efficiency. The collector costs 1.44 ms/sample today,
   which is fine at 1 Hz and 14% of a core at 10 Hz on a busy box — and `D1`
   wants a faster rate.
9. `D1` short-lived process capture. The largest real gap against atop.

## Sizing

`S` = an afternoon · `M` = a day or two · `L` = a week-plus, or needs its own plan.
