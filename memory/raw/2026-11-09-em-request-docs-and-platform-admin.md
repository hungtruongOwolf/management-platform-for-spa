# EM request — 2026-11-09: reorganize docs + README; a platform admin page

Verbatim:

> chạy chạy hết mọi thứ xong có thể sắp xếp lại docs và viết readme cho hợp lý cho tôi với được không từ system design, database design và cả các feature nữa cái system design và database design là high level in detail là được nhé
>
> bên cạnh đó hiện chúng ta chưa có page cho admin để quản lý mấy vụ feature, role, functions và cả accoutn các thứ có gì được thì implement luôn

The lead asked who the admin page is for (platform operator vs spa owner). The EM chose: **"Admin nền tảng"**, for the product operator when selling to many spas.

## Lead's reading
1. **Docs.** Once the current work (database plan) is merged, reorganize the documentation and write a proper README. It covers:
   - **system design**: high level, but detailed;
   - **database design**: high level, but detailed;
   - **features**: what the app does, by role and area.

   The wiki (`memory/`) stays the knowledge base. The README and a `docs/` entry point make it readable for a newcomer and link into the wiki rather than duplicating it.
2. **Platform admin console**, for the operator, separate from the spas' app. It manages:
   - spas (tenants);
   - features per spa (feature flags, enforced on the server);
   - roles and their permissions ("functions");
   - accounts: lock/unlock, owner password recovery (replacing the CLI path), MFA reset;
   - audit.

   Security first: a separate operator identity with mandatory MFA. Every action is audited, deny by default, and there is no silent RLS bypass.
