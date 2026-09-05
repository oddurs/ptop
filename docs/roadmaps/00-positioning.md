# Positioning

ptop's README and commit history claim it does something no other tool does.
That claim is wrong and needs correcting before anyone reads it.

---

## X1 — Correct the prior-art claims  ·  `S`

**What.** Rewrite the opening of `README.md` and the "Status" section so the
comparison to existing tools is accurate.

**Why.** The README currently says htop, btop and bottom "all show you *now*"
and implies nothing lets you rewind. Two counter-examples, both verified:

- **[atop](https://www.atoptool.nl/)** records per-process history to compressed
  daily logfiles (28 days by default) and replays it. It also captures processes
  that *exited during the interval* — the exact case ptop's pitch invokes and
  ptop cannot currently see. atop is the stronger tool on capability.
- **[zenith](https://github.com/bvaisvil/zenith)** has zoomable scroll-back
  charts and persists data between runs.

What survives scrutiny is narrower and still worth saying:

- zenith's scrollback is **charts only**. Verified in source: `HistogramKind`
  carries only aggregate series (`Cpu`, `Mem`, `NetTx`, `NetRx`, `IoRead`,
  `IoWrite`, `Gpu*`, `FileSystemUsedSpace`) — no per-process variant — and the
  process table renders from a live `process_map` that runs
  `.retain(|&k, _| current_pids.contains(&k))`, so exited processes are dropped
  and the table never follows the time cursor.
- atop needs its daemon to have been running **beforehand**. If it wasn't, there
  is nothing to replay. ptop gives you the last ten minutes the moment you
  launch it on a box you just connected to, with no daemon, config or logfiles.

So the honest pitch is **zero-setup, in-session scrubbing with per-instant
process tables** — a usability position against atop, not a capability one.

**Acceptance.**
- README names atop and zenith and states plainly what each already does.
- The claim made is the zero-setup one, with no "nothing else does this".
- The short-lived-process limitation (`D1`) is disclosed, not buried.

**Touches.** `README.md`

---

## X2 — Comparison table  ·  `S`

**What.** A table in the README covering ptop, htop, btop, bottom, zenith, atop
across: live view, historical charts, historical process table, exited-process
capture, cross-restart persistence, daemon required, zero-config.

**Why.** A reader deciding whether to use ptop deserves to find atop from
ptop's own README if atop is what they actually need. A comparison that admits
where you lose is more persuasive than one that doesn't, and it prevents the
overclaim in `X1` from creeping back.

**Acceptance.** Every row verified against source or official docs, not from
memory. Rows where ptop loses are present and marked.

**Depends on.** `X1`

**Touches.** `README.md`
