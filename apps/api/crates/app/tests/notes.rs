//! "Nhật ký ghi chú" (`GET /notes`, spec management-views §10): every note kind under its own
//! read rule, decryption only for entitled callers, technicians 403, tenant isolation, filters,
//! search over encrypted notes, cursor pagination and the default `private, no-store`.

mod common;

use axum::http::{StatusCode, header};
use chrono::{Duration, NaiveDate, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::testing::app_pool;
use kernel::{Actor, Permission, TenantId, UserId};
use reporting::notes_dto::{NoteKind, NotesQuery};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";
const PRICE: i64 = 150_000;
const PLAN_PRICE: i64 = 1_200_000;

const VISIT_NOTE: &str = "Đau vai gáy, ấn nhẹ";
const HEALTH_NOTE: &str = "Huyết áp cao, tránh xông nóng";
const PROFILE_NOTE: &str = "Thích phòng yên tĩnh";
const BOOKING_NOTE: &str = "Khách muốn phòng riêng";
const STAFF_NOTE: &str = "Đang học thêm khóa massage đá nóng";
const CANCEL_REASON: &str = "Khách bận việc đột xuất";
const NO_SHOW_REASON: &str = "Gọi không nghe máy";
const PACKAGE_NOTE: &str = "Mua gói vì đau thắt lưng";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    tech: String,
    tenant: Uuid,
    lan: Uuid,
    mai: Uuid,
    customer: Uuid,
    visit: Uuid,
}

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

fn id(reply: &Reply) -> Uuid {
    reply.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

async fn post(h: &Harness, session: &str, path: &str, body: Value) -> Reply {
    let k = Uuid::now_v7().to_string();
    call_with_headers(
        &h.app,
        "POST",
        path,
        Some(session),
        Some(body),
        &[("Idempotency-Key", &k)],
    )
    .await
}

/// One spa with a note of every kind written today.
async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Hà",
        SystemRole::Manager,
    )
    .await?;
    add_member(&h.owner, "vpyd", TECH_PHONE, "KTV", SystemRole::Technician).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let mai = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let category = call(
        &h.app,
        "POST",
        "/service-categories",
        Some(&owner),
        Some(json!({"name": "Dưỡng sinh"})),
    )
    .await;
    let service = call(
        &h.app,
        "POST",
        "/services",
        Some(&owner),
        Some(json!({
            "category_id": id(&category), "name": "Gội đầu dưỡng sinh", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": PRICE}
        })),
    )
    .await;
    assert_eq!(service.status, StatusCode::CREATED, "{}", service.body);
    let created = post_customer(
        &h.app,
        Some(&owner),
        Some(json!({
            "full_name": "Nguyễn Thị Hoa", "phone": "0987000111", "gender": "female",
            "note": PROFILE_NOTE, "health_note": HEALTH_NOTE
        })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let customer = id(&created);
    let line = json!({"service_id": id(&service), "source": "one_off", "staff_id": lan});
    let visit = post(
        &h,
        &owner,
        "/visits",
        json!({
            "customer_id": customer, "lines": [line], "note": VISIT_NOTE,
            "discount": {"amount": 15_000, "reason": "Khách quen giảm tại quầy"},
            "payments": [{"method": "cash", "amount": PRICE - 15_000}]
        }),
    )
    .await;
    assert_eq!(visit.status, StatusCode::CREATED, "{}", visit.body);
    let wrong = post(&h, &owner, "/visits", json!({
        "customer_id": customer, "lines": [line], "payments": [{"method": "cash", "amount": PRICE}]
    })).await;
    assert_eq!(wrong.status, StatusCode::CREATED, "{}", wrong.body);
    let void = post(
        &h,
        &owner,
        &format!("/visits/{}/void", id(&wrong)),
        json!({"reason": "Ghi nhầm khách"}),
    )
    .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);
    let follow = post(
        &h,
        &manager,
        &format!("/customers/{customer}/follow-ups"),
        json!({
            "channel": "phone", "outcome": "will_come", "note": "Hẹn tuần sau quay lại"
        }),
    )
    .await;
    assert_eq!(follow.status, StatusCode::CREATED, "{}", follow.body);
    // A package sold with a discount (not in a visit), later extended (below).
    let plan = call(&h.app, "POST", "/package-plans", Some(&owner), Some(json!({
        "name": "Gói 10 buổi", "price": PLAN_PRICE, "validity_months": 6, "gift_note": null,
        "active": true, "items": [{"service_id": id(&service), "sessions": 10, "bonus_sessions": 0}]
    }))).await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let sale = post(&h, &owner, "/sales", json!({
        "customer_id": customer, "closer_staff_id": lan, "lines": [{"package_plan_id": id(&plan)}],
        "discount": {"amount": 200_000, "reason": "Mua gói thứ hai giảm giá"},
        "payments": [{"method": "cash", "amount": PLAN_PRICE - 200_000}],
        "note": PACKAGE_NOTE
    })).await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);

    // The staff books, written straight to the tables (their own tests cover the HTTP paths).
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let manager_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912000111'")
            .fetch_one(&h.owner)
            .await?;
    let day = today();
    sqlx::query(
        "INSERT INTO app.attendance_day (tenant_id, staff_id, work_date, status, note, recorded_by)
         VALUES ($1, $2, $3, 'leave_dx', 'Con ốm, nghỉ đột xuất', $4)",
    )
    .bind(tenant)
    .bind(lan)
    .bind(day)
    .bind(manager_user)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.staff_daily_review (tenant_id, staff_id, work_date, compliments, complaints, note, recorded_by)
         VALUES ($1, $2, $3, 1, 0, 'Khách khen tay nghề', $4)",
    )
    .bind(tenant).bind(lan).bind(day).bind(manager_user)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.staff_adjustment (id, tenant_id, staff_id, work_date, kind, amount, reason, created_by)
         VALUES ($1, $2, $3, $4, 'bonus', 100000, 'Bán được gói lớn', $5)",
    )
    .bind(Uuid::now_v7()).bind(tenant).bind(lan).bind(day).bind(manager_user)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "UPDATE app.staff_employment SET ended_on = $3, end_reason = 'Chuyển về quê'
         WHERE tenant_id = $1 AND staff_id = $2 AND ended_on IS NULL",
    )
    .bind(tenant)
    .bind(mai)
    .bind(day)
    .execute(&h.owner)
    .await?;
    // The other new kinds, also straight to their tables.
    let owner_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84905123456'")
            .fetch_one(&h.owner)
            .await?;
    sqlx::query(
        "INSERT INTO app.customer_package_extension
           (id, tenant_id, customer_package_id, previous_expires_on, expires_on, reason, business_date, created_by)
         SELECT $1, cp.tenant_id, cp.id, $3::date + 180, $3::date + 210, 'Khách ốm, gia hạn một tháng', $3, $4
         FROM app.customer_package cp WHERE cp.tenant_id = $2 AND cp.customer_id = $5",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(day)
    .bind(owner_user)
    .bind(customer)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.appointment
           (id, tenant_id, branch_id, customer_id, business_date, starts_at, ends_at, note, created_by)
         SELECT $1, $2, b.id, $3, $4::date + 1, now() + interval '1 day', now() + interval '25 hours', $5, $6
         FROM app.branch b WHERE b.tenant_id = $2 LIMIT 1",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(customer)
    .bind(day)
    .bind(BOOKING_NOTE)
    .bind(manager_user)
    .execute(&h.owner)
    .await?;
    // F21: why a booking was cancelled (the customer's) or the person did not come (a guest's).
    sqlx::query(
        "INSERT INTO app.appointment
           (id, tenant_id, branch_id, customer_id, guest_name, guest_phone, business_date, starts_at,
            ends_at, status, status_reason, closed_at, closed_by, created_by)
         SELECT v.id, $2, b.id, v.customer_id, v.guest_name, v.guest_phone, $3::date + 2,
                now() + interval '2 days', now() + interval '49 hours', v.status, v.reason, now(), $4, $4
         FROM app.branch b,
              (VALUES ($1::uuid, $5::uuid, NULL::text, NULL::text, 'cancelled', $6::text),
                      ($7::uuid, NULL::uuid, 'Khách Yến', '+84905000111', 'no_show', $8::text))
                AS v (id, customer_id, guest_name, guest_phone, status, reason)
         WHERE b.tenant_id = $2",
    )
    .bind(Uuid::now_v7())
    .bind(tenant)
    .bind(day)
    .bind(manager_user)
    .bind(customer)
    .bind(CANCEL_REASON)
    .bind(Uuid::now_v7())
    .bind(NO_SHOW_REASON)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.staff_evaluation
           (tenant_id, staff_id, work_date, criteria, total_points, result_level, coaching_plan, comment, recorded_by)
         VALUES ($1, $2, date_trunc('month', $3::date - 31)::date, '[]'::jsonb, 70, 'm3',
                 'Kèm hai tuần kỹ thuật bấm huyệt', 'Cần nhẹ tay hơn', $4)",
    )
    .bind(tenant)
    .bind(lan)
    .bind(day)
    .bind(manager_user)
    .execute(&h.owner)
    .await?;
    sqlx::query("UPDATE app.staff SET note = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(lan)
        .bind(STAFF_NOTE)
        .execute(&h.owner)
        .await?;
    Ok(Spa {
        owner,
        manager,
        tech,
        tenant,
        lan,
        mai,
        customer,
        visit: id(&visit),
        h,
    })
}

impl Spa {
    async fn notes(&self, session: &str, query: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            &format!("/notes{query}"),
            Some(session),
            None,
        )
        .await
    }

    /// The service as a caller holding exactly `permissions`.
    async fn as_actor(
        &self,
        permissions: &[Permission],
        query: NotesQuery,
    ) -> Setup<reporting::notes_dto::NotesPage> {
        let actor = Actor::new(
            UserId::new(),
            TenantId::from_uuid(self.tenant),
            permissions.iter().copied(),
            None,
        );
        let app = app_pool(&self.h.owner).await?;
        Ok(reporting::notes::list(&app, &self.h.cipher, &actor, &query).await?)
    }
}

fn query() -> NotesQuery {
    NotesQuery {
        from: None,
        to: None,
        kind: None,
        staff_id: None,
        customer_id: None,
        q: None,
        cursor: None,
        limit: Some(100),
    }
}

fn kinds_of(body: &Value) -> BTreeSet<String> {
    body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["kind"].as_str().map(str::to_owned))
        .collect()
}

fn texts(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["text"].as_str().map(str::to_owned))
        .collect()
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn each_role_reads_exactly_the_notes_it_may(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let all: BTreeSet<String> = NoteKind::ALL
        .iter()
        .map(|k| k.as_str().to_owned())
        .collect();

    let owner = s.notes(&s.owner, "").await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(kinds_of(&owner.body), all, "the owner reads every kind");
    let text = texts(&owner.body);
    for t in [
        VISIT_NOTE,
        HEALTH_NOTE,
        PROFILE_NOTE,
        "Ghi nhầm khách",
        "Chuyển về quê",
        "Hẹn tuần sau quay lại",
        BOOKING_NOTE,
        "Cần nhẹ tay hơn",
        "Kèm hai tuần kỹ thuật bấm huyệt",
        STAFF_NOTE,
        "Khách ốm, gia hạn một tháng",
        "Khách quen giảm tại quầy",
        "Mua gói thứ hai giảm giá",
        CANCEL_REASON,
        NO_SHOW_REASON,
        PACKAGE_NOTE,
    ] {
        assert!(text.iter().any(|x| x == t), "owner sees {t}");
    }
    // F4: "Ghi chú gói" names the package and its owner.
    let package_note = owner.body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|n| n["kind"] == "package_note")
        .cloned()
        .unwrap_or_default();
    assert_eq!(package_note["text"], PACKAGE_NOTE);
    assert_eq!(package_note["detail"], "Gói 10 buổi");
    assert_eq!(package_note["customer_id"], s.customer.to_string());
    assert_eq!(package_note["customer_name"], "Nguyễn Thị Hoa");
    // F21: the reason says which close it was, for which day, about whom, and who closed it.
    let reasons: Vec<Value> = owner.body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|n| n["kind"] == "appointment_reason")
        .cloned()
        .collect();
    let of = |detail: &str| {
        reasons
            .iter()
            .find(|n| n["detail"] == detail)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(of("cancelled")["text"], CANCEL_REASON);
    assert_eq!(of("cancelled")["customer_name"], "Nguyễn Thị Hoa");
    assert_eq!(of("no_show")["customer_name"], "Khách Yến");
    assert_eq!(of("no_show")["author_name"], "Quản lý Hà");
    assert_eq!(
        of("no_show")["about_date"],
        (today() + Duration::days(2)).to_string()
    );
    // Who wrote it and about whom.
    let follow = owner.body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|n| n["kind"] == "follow_up")
        .cloned()
        .unwrap_or_default();
    assert_eq!(follow["author_name"], "Quản lý Hà");
    assert_eq!(follow["customer_name"], "Nguyễn Thị Hoa");
    assert_eq!(follow["customer_id"], s.customer.to_string());
    let visit = owner.body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|n| n["kind"] == "visit_note")
        .cloned()
        .unwrap_or_default();
    assert_eq!(visit["visit_id"], s.visit.to_string());
    // Default: the last 30 days, today included.
    assert_eq!(owner.body["to"], today().to_string());
    assert_eq!(
        owner.body["from"],
        (today() - Duration::days(29)).to_string()
    );
    // The default no-store from the middleware.
    assert_eq!(
        owner
            .headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("private, no-store")
    );

    // Manager: everything but the owner's HR texts (end-of-employment reasons and the profile's
    // HR note, both outside the manager's 14-key roster allowlist).
    let manager = s.notes(&s.manager, "").await;
    assert_eq!(manager.status, StatusCode::OK, "{}", manager.body);
    let mut expected = all.clone();
    expected.remove("employment_end");
    expected.remove("staff_note");
    assert_eq!(kinds_of(&manager.body), expected);
    assert!(!manager.body.to_string().contains("Chuyển về quê"));
    assert!(!manager.body.to_string().contains(STAFF_NOTE));
    for owner_only in ["employment_end", "staff_note"] {
        assert!(
            !manager.body["kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| k == owner_only)
        );
    }

    // Technicians never; anonymous callers are not signed in.
    let tech = s.notes(&s.tech, "").await;
    assert_eq!(tech.status, StatusCode::FORBIDDEN, "{}", tech.body);
    assert_eq!(
        call(&s.h.app, "GET", "/notes", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Reading writes nothing: no audit row carries a note.
    let leaks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE after::text LIKE '%vai gáy%' OR after::text LIKE '%Huyết áp%'
                                              OR after::text LIKE '%thắt lưng%'",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(leaks, 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_manager_without_the_health_permission_sees_no_health_notes(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // The seeded manager role minus customer.health_notes.read.
    let manager: Vec<Permission> = SystemRole::Manager
        .default_permissions()
        .into_iter()
        .filter(|p| *p != Permission::CustomerHealthNotesRead)
        .collect();
    let page = s.as_actor(&manager, query()).await?;
    let kinds: BTreeSet<NoteKind> = page.items.iter().map(|n| n.kind).collect();
    assert!(
        !kinds.contains(&NoteKind::VisitNote)
            && !kinds.contains(&NoteKind::HealthNote)
            && !kinds.contains(&NoteKind::PackageNote),
        "{kinds:?}"
    );
    assert!(kinds.contains(&NoteKind::CustomerNote) && kinds.contains(&NoteKind::Void));
    assert!(
        !page.kinds.contains(&NoteKind::HealthNote)
            && !page.kinds.contains(&NoteKind::VisitNote)
            && !page.kinds.contains(&NoteKind::PackageNote)
    );
    let all = serde_json::to_string(&page)?;
    assert!(!all.contains("vai gáy") && !all.contains("Huyết áp") && !all.contains("thắt lưng"));
    // Asking for the kind or searching its words does not open it either.
    let only = s
        .as_actor(
            &manager,
            NotesQuery {
                kind: Some(NoteKind::HealthNote),
                ..query()
            },
        )
        .await?;
    assert!(only.items.is_empty());
    let searched = s
        .as_actor(
            &manager,
            NotesQuery {
                q: Some("huyet ap".into()),
                ..query()
            },
        )
        .await?;
    assert!(searched.items.is_empty(), "{:?}", searched.items);
    let searched = s
        .as_actor(
            &manager,
            NotesQuery {
                q: Some("that lung".into()),
                ..query()
            },
        )
        .await?;
    assert!(searched.items.is_empty(), "{:?}", searched.items);
    Ok(())
}

/// Each kind opens with exactly the permissions it already needs where it was written.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_kind_keeps_its_own_read_rule(pool: PgPool) -> TestResult {
    use Permission as P;
    let s = spa(pool).await?;
    let rules: [(NoteKind, &[Permission]); 15] = [
        (NoteKind::AppointmentNote, &[P::AppointmentManage]),
        (NoteKind::AppointmentReason, &[P::AppointmentManage]),
        (NoteKind::EvaluationNote, &[P::EvaluationRecord]),
        (NoteKind::StaffNote, &[P::StaffManage]),
        (
            NoteKind::PackageExtension,
            &[P::CustomerRead, P::VisitRead, P::SaleRead],
        ),
        (
            NoteKind::VisitNote,
            &[P::VisitRead, P::CustomerHealthNotesRead],
        ),
        (NoteKind::CustomerNote, &[P::CustomerRead]),
        (
            NoteKind::HealthNote,
            &[P::CustomerRead, P::CustomerHealthNotesRead],
        ),
        (NoteKind::AttendanceNote, &[P::AttendanceRecord]),
        (NoteKind::ReviewNote, &[P::EvaluationRecord]),
        (NoteKind::Adjustment, &[P::EvaluationRecord]),
        (NoteKind::FollowUp, &[P::CustomerRead]),
        (
            NoteKind::Void,
            &[P::CustomerRead, P::VisitRead, P::SaleRead],
        ),
        (NoteKind::EmploymentEnd, &[P::StaffManage]),
        (
            NoteKind::PackageNote,
            &[P::SaleRead, P::CustomerRead, P::CustomerHealthNotesRead],
        ),
    ];
    for (kind, needs) in rules {
        let mut granted = vec![P::ReportRead];
        granted.extend_from_slice(needs);
        let page = s.as_actor(&granted, query()).await?;
        assert!(
            page.items.iter().any(|n| n.kind == kind),
            "{kind:?} with {needs:?}"
        );
        // Drop any one of the permissions it needs: the kind is gone.
        for missing in needs {
            let fewer: Vec<Permission> = granted.iter().copied().filter(|p| p != missing).collect();
            let page = s.as_actor(&fewer, query()).await?;
            assert!(
                !page.items.iter().any(|n| n.kind == kind),
                "{kind:?} without {missing:?}"
            );
            assert!(!page.kinds.contains(&kind));
        }
    }
    // Discounts: a visit's under visit.read (the day list), any other sale's under sale.read +
    // customer.read (GET /sales/{id} + the name); sale.read alone shows none.
    let details = |page: &reporting::notes_dto::NotesPage| -> BTreeSet<String> {
        page.items
            .iter()
            .filter(|n| n.kind == NoteKind::Discount)
            .filter_map(|n| n.detail.clone())
            .collect()
    };
    let on_visit = s.as_actor(&[P::ReportRead, P::VisitRead], query()).await?;
    assert_eq!(details(&on_visit), BTreeSet::from(["visit".to_owned()]));
    let on_sale = s
        .as_actor(&[P::ReportRead, P::SaleRead, P::CustomerRead], query())
        .await?;
    assert_eq!(details(&on_sale), BTreeSet::from(["sale".to_owned()]));
    let bare = s.as_actor(&[P::ReportRead, P::SaleRead], query()).await?;
    assert!(details(&bare).is_empty() && !bare.kinds.contains(&NoteKind::Discount));
    // report.read alone: the screen opens, empty.
    assert!(
        s.as_actor(&[P::ReportRead], query())
            .await?
            .items
            .is_empty()
    );
    // Without report.read: 403 whatever else is held.
    let actor = Actor::new(
        UserId::new(),
        TenantId::from_uuid(s.tenant),
        [P::CustomerRead, P::StaffManage],
        None,
    );
    let app = app_pool(&s.h.owner).await?;
    let denied = reporting::notes::list(&app, &s.h.cipher, &actor, &query()).await;
    assert!(
        matches!(denied, Err(kernel::AppError::Forbidden { .. })),
        "{denied:?}"
    );
    // ADR-0014: a manager whose staff profile has left keeps only the former-staff permissions,
    // so no kind (new ones included) is reachable.
    let former =
        kernel::permission::effective_permissions(SystemRole::Manager.default_permissions(), true);
    let actor = Actor::new(UserId::new(), TenantId::from_uuid(s.tenant), former, None);
    let denied = reporting::notes::list(&app, &s.h.cipher, &actor, &query()).await;
    assert!(
        matches!(denied, Err(kernel::AppError::Forbidden { .. })),
        "{denied:?}"
    );
    Ok(())
}

/// ADR-0015 (lead 2026-11-08): the HR notes about the caller's own linked profile (attendance,
/// praise / complaints, bonus / penalty, monthly evaluation) are not in a manager's journal: they
/// read their own through `/me/*`. The owner still reads every manager's. Notes the manager wrote
/// about others stay.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_manager_does_not_read_notes_about_their_own_profile(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let manager_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912000111'")
            .fetch_one(&s.h.owner)
            .await?;
    let own = org::provisioning::add_staff(&s.h.owner, "vpyd", "Quản lý Hà", "Quản lý").await?;
    sqlx::query("UPDATE app.staff SET user_id = $2 WHERE id = $1")
        .bind(own)
        .bind(manager_user)
        .execute(&s.h.owner)
        .await?;
    let owner_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84905123456'")
            .fetch_one(&s.h.owner)
            .await?;
    let day = today();
    sqlx::query(
        "INSERT INTO app.attendance_day (tenant_id, staff_id, work_date, status, note, recorded_by)
         VALUES ($1, $2, $3, 'present', 'Quản lý đến muộn vì kẹt xe', $4)",
    )
    .bind(s.tenant)
    .bind(own)
    .bind(day)
    .bind(owner_user)
    .execute(&s.h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.staff_adjustment (id, tenant_id, staff_id, work_date, kind, amount, reason, created_by)
         VALUES ($1, $2, $3, $4, 'penalty', 50000, 'Quên khóa két', $5)",
    )
    .bind(Uuid::now_v7())
    .bind(s.tenant)
    .bind(own)
    .bind(day)
    .bind(owner_user)
    .execute(&s.h.owner)
    .await?;

    let manager = s.notes(&s.manager, "").await;
    assert_eq!(manager.status, StatusCode::OK, "{}", manager.body);
    let text = texts(&manager.body);
    for own_note in ["Quản lý đến muộn vì kẹt xe", "Quên khóa két"] {
        assert!(
            !text.iter().any(|t| t == own_note),
            "manager must not see {own_note}"
        );
    }
    assert!(
        text.iter().any(|t| t == "Con ốm, nghỉ đột xuất"),
        "notes about the people they look after stay"
    );
    // Filtering on their own profile still shows what they wrote about others, nothing about them.
    let filtered = s.notes(&s.manager, &format!("?staff_id={own}")).await;
    assert!(
        !filtered.body["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["staff_id"] == own.to_string()),
        "{}",
        filtered.body
    );
    let owner = texts(&s.notes(&s.owner, "").await.body);
    for own_note in ["Quản lý đến muộn vì kẹt xe", "Quên khóa két"] {
        assert!(
            owner.iter().any(|t| t == own_note),
            "the owner sees {own_note}"
        );
    }
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn filters_search_and_pages_with_a_cursor(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let only = |body: &Value| kinds_of(body).into_iter().collect::<Vec<_>>();

    assert_eq!(only(&s.notes(&s.owner, "?kind=void").await.body), ["void"]);
    // A customer: their profile, health, visit, void and follow-up notes; no staff book.
    let mine = s
        .notes(&s.owner, &format!("?customer_id={}", s.customer))
        .await;
    assert_eq!(
        kinds_of(&mine.body),
        [
            "appointment_note",
            "appointment_reason",
            "customer_note",
            "discount",
            "follow_up",
            "health_note",
            "package_extension",
            // F4: the note of the package she bought.
            "package_note",
            "visit_note",
            "void"
        ]
        .map(String::from)
        .into()
    );
    // A staff member: what is about them, plus the visits they served.
    let lan = s.notes(&s.owner, &format!("?staff_id={}", s.lan)).await;
    assert_eq!(
        kinds_of(&lan.body),
        [
            "adjustment",
            "attendance_note",
            "discount",
            "evaluation_note",
            "review_note",
            "staff_note",
            "visit_note",
            "void"
        ]
        .map(String::from)
        .into()
    );
    let mai = s.notes(&s.owner, &format!("?staff_id={}", s.mai)).await;
    assert_eq!(only(&mai.body), ["employment_end"]);
    // Search: plain notes in SQL, encrypted ones after decryption, accents ignored.
    assert_eq!(
        texts(&s.notes(&s.owner, "?q=tuan%20sau").await.body),
        ["Hẹn tuần sau quay lại"]
    );
    assert_eq!(
        texts(&s.notes(&s.owner, "?q=VAI%20GAY").await.body),
        [VISIT_NOTE]
    );
    let by_name = s.notes(&s.owner, "?q=thi%20hoa&kind=health_note").await;
    assert_eq!(texts(&by_name.body), [HEALTH_NOTE]);
    assert_eq!(
        s.notes(&s.owner, "?q=a").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Date range: nothing was written yesterday; ranges are bounded.
    let yesterday = (today() - Duration::days(1)).to_string();
    let none = s
        .notes(&s.owner, &format!("?from={yesterday}&to={yesterday}"))
        .await;
    assert_eq!(none.status, StatusCode::OK, "{}", none.body);
    assert!(texts(&none.body).is_empty());
    let long = s.notes(&s.owner, "?from=2025-01-01&to=2026-06-01").await;
    assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        s.notes(&s.owner, "?cursor=bm9wZQ").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.notes(&s.owner, "?userId=x").await.status,
        StatusCode::BAD_REQUEST
    );

    // Pages of 2 walk the whole list once, newest first, no repeats.
    let full = texts(&s.notes(&s.owner, "").await.body);
    let mut walked = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let path = cursor
            .as_ref()
            .map_or("?limit=2".to_owned(), |c| format!("?limit=2&cursor={c}"));
        let page = s.notes(&s.owner, &path).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        assert!(texts(&page.body).len() <= 2);
        walked.extend(texts(&page.body));
        cursor = page.body["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(walked, full);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spa_sees_none_of_our_notes(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    create_tenant(&s.h.owner, "khac", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "khac",
        OTHER_OWNER_PHONE,
        "Chủ spa khác",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, OTHER_OWNER_PHONE).await;
    let page = s.notes(&other, "").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    assert!(texts(&page.body).is_empty(), "{}", page.body);
    let aimed = s
        .notes(
            &other,
            &format!("?customer_id={}&staff_id={}", s.customer, s.lan),
        )
        .await;
    assert!(texts(&aimed.body).is_empty(), "{}", aimed.body);
    Ok(())
}
