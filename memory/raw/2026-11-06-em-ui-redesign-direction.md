# EM decision — 2026-11-06: UI redesign direction (after round-5 fixes)

Verbatim (EM, chat):

> https://github.com/Leonxlnx/taste-skill
> https://github.com/pbakaus/impeccable
> sau khi làm xong thì bạn đọc 2 skill này giúp tôi design lại UI và mọi thứ nhé chứ tôi thấy hiện tại nó tệ lắm ấy để cho UI/UX đẹp hơn ok hơn xịn sò hơn, hợp lý hơn, logic hơn

Lead asked 4 questions; the EM answered:

> ok theo đề xuất nhưng không cần ghi nguồn đâu cứ ấy mà dùng là được

## Decisions (lead's proposals, accepted)
1. Lora only at brand spots (logo, login, welcome band). Work-screen titles use the body font.
2. Remove the uppercase eyebrows, corner ornaments and dot texture from work screens.
3. Remove `CountUp` and the gradient/coloured shadow on primary buttons. Primary buttons become flat.
4. Write the rules into our own `design-system.md` and the `frontend-design` / `pr-review` skills, with no source attribution needed (EM). Don't vendor the upstream files or scripts. Research: [2026-11-06-research-ui-skills-taste-impeccable.md](2026-11-06-research-ui-skills-taste-impeccable.md).

Sequence: finish the round-5 PRs (visits table/sidebar, appointments, staff) first, then the redesign. Show the EM 2–3 sample screens before the bulk change.
