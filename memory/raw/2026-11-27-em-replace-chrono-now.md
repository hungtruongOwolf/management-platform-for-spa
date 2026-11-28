---
source: EM, chat 2026-11-27 (answer to the lead's question)
---

The lead asked how to handle the red `cargo-deny` step: `chrono` and `chrono-tz` are flagged unmaintained (RUSTSEC-2026-0543, RUSTSEC-2026-0555, informational, no patched version; recommended alternatives `jiff`, `time`). It blocks every PR that touches Rust, `main` included.

Options offered:

1. Ignore both IDs in `apps/api/deny.toml` with a reason and a review date, and plan a later move to `jiff` / `time` (lead's recommendation).
2. Replace chrono now: the whole API moves to `jiff` or `time`, about 96 files; the lucky draw waits.
3. Downgrade every "unmaintained" advisory to a warning.

EM's answer: **"Thay chrono ngay"** (option 2).
