---
name: secure
description: Security engineering — apply when code or design touches authentication, authorization, sessions, user or health data, payments, file uploads, video/media delivery, or premium content; also for threat modeling and security review.
---

# Secure

Mindset: **assume the attacker owns the browser.** They read every byte of JS, edit any request in DevTools, replay it with curl, call endpoints the UI never shows, and run scripts that download everything an authenticated user can reach. A control that runs only in the client is decoration. Every protection that matters is enforced on the server, and every leak is designed to be traceable.

## Steps
1. **Assets & boundaries.** Name what is being protected (accounts, personal/health data, paid video, documents, payments, admin powers) and every point where untrusted input crosses into the system.
2. **Apply the checklists** for every branch the change touches — each item is satisfied, not applicable with a reason, or a finding:
   - Identity, sessions, roles, ownership → [authn-authz.md](authn-authz.md)
   - Any HTTP endpoint, form, upload, header, dependency, log → [web-checklist.md](web-checklist.md)
   - Video, audio, images, documents, any paid or member-only content → [content-protection.md](content-protection.md)
3. **Abuse cases.** For each endpoint changed, write how a logged-in attacker would misuse it: change an ID (IDOR), change role/price/flag in the body, replay, skip a step in a flow, call it at 1000 req/s, call it as another tenant. Each abuse case has a server-side control and a test.
4. **Report.** Findings by severity (Critical / High / Medium / Low): location, attack scenario, fix. Critical and High block "done".
5. **Record.** Update `memory/wiki/security/` (threat model, authz matrix, content-protection plan) through the `memory` skill.

Done when every checklist item touched is resolved and every abuse case has a control plus a test.
