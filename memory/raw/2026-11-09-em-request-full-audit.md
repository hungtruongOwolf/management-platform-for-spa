# EM request — 2026-11-09: full audit of every doc, file and piece of code

Verbatim:

> bạn check kĩ lại từ đầu đến cuối cho tôi tất cả các docs, các file, từng phần code 1 đừng để sai cái gì cả nhé mọi thứ phải đúng cái gì stale, cái gì cũ không cũng nữa thì xóa bỏ đi cái gì cải thiện được thì cải thiện làm kĩ cho tôi

Context: right after the docs reorganization (#114) and a Mermaid parse error the EM found on GitHub (fixed in #116).

## Lead's reading
- Review everything, end to end: docs (README, docs/, memory/wiki), repo files (scripts, CI, infra, config), and all code (Rust API crates and migrations, web app, api-client, e2e). Nothing may be wrong.
- Stale or obsolete things that are no longer used get deleted:
  - tracked files, dead code and outdated wiki content;
  - never `memory/raw/` (immutable) or the EM's own untracked files.
- Improve what can be improved:
  - bugs and security gaps get fixed, with tests;
  - simplifications that don't change behaviour;
  - wrong docs get corrected against the code.
  - Product-behaviour changes are not made silently; they become questions for the EM.
- Split into parallel agents by area. The machine is low on RAM, so CI verifies Rust and there are no parallel local cargo builds.
