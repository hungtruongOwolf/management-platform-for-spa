//! Staff management (spec staff-management): salary visibility, technician self-service,
//! tenant isolation, the payroll close lock, commission reconciling with visits, rules, audit.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, Reply, TestResult, add_member, call, call_with_headers, harness, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::testing::app_pool;
use kernel::{TenantId, TenantTx};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const OTHER_TENANT: &str = "other";
const MANAGER_PHONE: &str = "0912100111";
const TECH_PHONE: &str = "0912100222";
const OTHER_OWNER_PHONE: &str = "0913100333";
const PRICE: i64 = 150_000;
const PLAN_PRICE: i64 = 1_200_000;

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    tech: String,
    other: String,
    tenant: Uuid,
    other_tenant: Uuid,
    /// Linked to the technician's login.
    lan: Uuid,
    mai: Uuid,
    service: Uuid,
    plan: Uuid,
}

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

fn yesterday() -> NaiveDate {
    today() - Duration::days(1)
}

fn ym(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

/// Last day of the previous month.
fn end_of_last_month() -> NaiveDate {
    let t = today();
    t - Duration::days(i64::from(t.day()))
}

fn id(reply: &Reply) -> Uuid {
    reply.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

/// A scorecard with every criterion typed at the sheet's anchor of `level` (Mức 1 = 100 %,
/// Mức 2 = 80 %, Mức 3 = 60 % of its maximum).
fn criteria(level: &str) -> Value {
    let percent = match level {
        "m1" => 100,
        "m2" => 80,
        _ => 60,
    };
    json!(
        [
            ("technique", 40),
            ("attitude", 15),
            ("customer_feedback", 15),
            ("diligence", 10),
            ("hygiene", 10),
            ("learning", 10)
        ]
        .iter()
        .map(|(k, max)| json!({"key": k, "points": max * percent / 100}))
        .collect::<Vec<_>>()
    )
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Hoa",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    create_tenant(&h.owner, OTHER_TENANT, "Spa khác", "Huế").await?;
    add_member(
        &h.owner,
        OTHER_TENANT,
        OTHER_OWNER_PHONE,
        "Chủ khác",
        SystemRole::Owner,
    )
    .await?;
    let owner = session_for(&h, common::OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let other = session_for(&h, OTHER_OWNER_PHONE).await;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let other_tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = $1")
        .bind(OTHER_TENANT)
        .fetch_one(&h.owner)
        .await?;
    let tech_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912100222'")
            .fetch_one(&h.owner)
            .await?;

    let lan = call(
        &h.app,
        "POST",
        "/staff",
        Some(&owner),
        Some(json!({
            "full_name": "Lê Thị Lan", "position": "Kỹ thuật viên", "user_id": tech_user,
            "shift_start": "09:00"
        })),
    )
    .await;
    assert_eq!(lan.status, StatusCode::CREATED, "{}", lan.body);
    let mai = call(
        &h.app,
        "POST",
        "/staff",
        Some(&owner),
        Some(json!({
            "full_name": "Trần Thị Mai", "position": "Kỹ thuật viên"
        })),
    )
    .await;
    assert_eq!(mai.status, StatusCode::CREATED, "{}", mai.body);

    let c = call(
        &h.app,
        "POST",
        "/service-categories",
        Some(&owner),
        Some(json!({"name": "Gội đầu dưỡng sinh"})),
    )
    .await;
    let s = call(
        &h.app,
        "POST",
        "/services",
        Some(&owner),
        Some(json!({
            "category_id": id(&c), "name": "Gội đầu thảo dược", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": PRICE}
        })),
    )
    .await;
    assert_eq!(s.status, StatusCode::CREATED, "{}", s.body);
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gói 10 buổi", "price": PLAN_PRICE, "validity_months": null, "gift_note": null,
            "active": true, "items": [{"service_id": id(&s), "sessions": 10, "bonus_sessions": 2}]
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    Ok(Spa {
        owner,
        manager,
        tech,
        other,
        tenant,
        other_tenant,
        lan: id(&lan),
        mai: id(&mai),
        service: id(&s),
        plan: id(&plan),
        h,
    })
}

impl Spa {
    async fn get(&self, session: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(session), None).await
    }

    async fn put(&self, session: &str, path: &str, body: Value) -> Reply {
        call(&self.h.app, "PUT", path, Some(session), Some(body)).await
    }

    async fn post_key(&self, session: &str, path: &str, body: Value, key: &str) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(session),
            Some(body),
            &[("Idempotency-Key", key)],
        )
        .await
    }

    async fn post(&self, session: &str, path: &str, body: Value) -> Reply {
        self.post_key(session, path, body, &Uuid::now_v7().to_string())
            .await
    }

    async fn pay(&self, staff: Uuid, base: i64, rate_bp: i32) {
        let r = self
            .put(
                &self.owner,
                &format!("/staff/{staff}/pay"),
                json!({
                    "base_salary": base, "commission_rate_bp": rate_bp,
                    "loyalty_bonus_monthly": 500_000, "meal_allowance": 300_000
                }),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }

    async fn attend(&self, session: &str, staff: Uuid, day: NaiveDate, status: &str) -> Reply {
        let mut entry = json!({"staff_id": staff, "date": day, "status": status});
        if status == "present" {
            entry["check_in"] = json!("09:12");
        }
        self.put(session, "/attendance", json!({"entries": [entry]}))
            .await
    }

    async fn customer(&self, phone: &str) -> Uuid {
        let r = call(
            &self.h.app,
            "POST",
            "/customers",
            Some(&self.owner),
            Some(json!({
                "full_name": "Nguyễn Thị Hằng", "phone": phone, "gender": "female"
            })),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        id(&r)
    }

    async fn one_off_visit(&self, customer: Uuid, staff: Uuid, qty: i64) -> Uuid {
        let r = self.post(&self.owner, "/visits", json!({
            "customer_id": customer,
            "lines": [{"service_id": self.service, "source": "one_off", "staff_id": staff, "qty": qty}],
            "payments": [{"method": "cash", "amount": PRICE * qty}]
        })).await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        id(&r)
    }
}

fn slip(payroll: &Value, staff: Uuid) -> &Value {
    payroll["payslips"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["staff_id"] == json!(staff)))
        .unwrap_or(&Value::Null)
}

/// Non-negotiable 4: a caller without `payroll.read` never receives a salary figure, even when a
/// custom role lets them manage profiles. Mutation-checked: dropping the `payroll.read` checks
/// in org::profile (view + get) turns this red.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn salary_is_only_sent_to_payroll_readers(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    let own = s.get(&s.owner, &format!("/staff/{}", s.lan)).await;
    assert_eq!(own.status, StatusCode::OK);
    assert_eq!(own.body["pay"]["base_salary"], 5_000_000);
    assert_eq!(
        own.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    // A custom grant: the manager may now manage profiles, still without payroll.read.
    sqlx::query(
        "INSERT INTO app.role_permission (tenant_id, role_id, permission)
         SELECT tenant_id, id, 'staff.manage' FROM app.role WHERE tenant_id = $1 AND key = 'manager'",
    )
    .bind(s.tenant)
    .execute(&s.h.owner)
    .await?;
    for path in [format!("/staff/{}", s.lan), "/staff/profiles".to_owned()] {
        let r = s.get(&s.manager, &path).await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {}", r.body);
        let text = r.body.to_string();
        for key in [
            "base_salary",
            "commission_rate_bp",
            "meal_allowance",
            "\"pay\"",
        ] {
            assert!(!text.contains(key), "{path} leaked {key}: {text}");
        }
    }
    assert_eq!(
        s.get(&s.manager, &format!("/staff/{}", s.lan)).await.body["pay_visible"],
        false
    );
    let set = s
        .put(
            &s.manager,
            &format!("/staff/{}/pay", s.lan),
            json!({"base_salary": 1, "commission_rate_bp": 0}),
        )
        .await;
    assert_eq!(set.status, StatusCode::FORBIDDEN);
    for path in ["/payroll", "/payroll/rules"] {
        assert_eq!(
            s.get(&s.manager, path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let slip = s
        .get(&s.manager, &format!("/payroll/payslips/{}", s.lan))
        .await;
    assert_eq!(slip.status, StatusCode::FORBIDDEN);
    // The manager keeps the daily books.
    assert_eq!(
        s.attend(&s.manager, s.lan, yesterday(), "present")
            .await
            .status,
        StatusCode::OK
    );
    let mine = s.get(&s.manager, "/me/staff").await;
    assert_eq!(
        (mine.status, mine.body["linked"].clone()),
        (StatusCode::OK, json!(false))
    );
    Ok(())
}

/// A technician sees only their own month: the staff id comes from the session.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_sees_only_their_own_month(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    s.pay(s.mai, 7_777_000, 1000).await;
    for staff in [s.lan, s.mai] {
        assert_eq!(
            s.attend(&s.owner, staff, yesterday(), "present")
                .await
                .status,
            StatusCode::OK
        );
    }
    let r = s.post(&s.owner, "/adjustments", json!({
        "staff_id": s.mai, "kind": "penalty", "amount": 50_000, "reason": "Đi muộn nhiều lần"
    })).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);

    let me = s.get(&s.tech, "/me/staff").await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    assert_eq!(me.body["linked"], true);
    let data = &me.body["data"];
    assert_eq!(data["attendance"]["staff_id"], json!(s.lan));
    assert_eq!(data["attendance"]["days"].as_array().map(Vec::len), Some(1));
    assert_eq!(data["attendance"]["days"][0]["late_minutes"], 12);
    assert_eq!(data["payslip"]["staff_id"], json!(s.lan));
    assert_eq!(data["payslip"]["base_salary"], 5_000_000);
    let text = me.body.to_string();
    assert!(
        !text.contains(&s.mai.to_string())
            && !text.contains("Trần Thị Mai")
            && !text.contains("7777000"),
        "another person's data leaked: {text}"
    );

    // No way to ask for someone else.
    let other = s
        .get(&s.tech, &format!("/me/staff?staff_id={}", s.mai))
        .await;
    assert!(other.status.is_client_error(), "{}", other.status);
    for path in [
        "/attendance".to_owned(),
        "/daily-reviews".to_owned(),
        "/evaluations".to_owned(),
        "/adjustments".to_owned(),
        "/payroll".to_owned(),
        format!("/payroll/payslips/{}", s.mai),
        "/staff/profiles".to_owned(),
    ] {
        assert_eq!(
            s.get(&s.tech, &path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(
        s.attend(&s.tech, s.lan, yesterday(), "absent_k")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    Ok(())
}

/// Sorted keys of a JSON object (allowlist assertions).
fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    k.sort();
    k
}

/// Two linked technicians with work on a shared visit, their own visits and bookings: what the
/// self-service tests below look at.
struct Work {
    tech_b: String,
    thu: Uuid,
    hang: Uuid,
    binh: Uuid,
    shared: Uuid,
    b_only: Uuid,
    lan_own: Uuid,
    book_a: Value,
}

async fn two_technicians_at_work(s: &Spa) -> Setup<Work> {
    const TECH_B_PHONE: &str = "0912100444";
    add_member(
        &s.h.owner,
        "vpyd",
        TECH_B_PHONE,
        "KTV Thu",
        SystemRole::Technician,
    )
    .await?;
    let tech_b = session_for(&s.h, TECH_B_PHONE).await;
    // Not linked yet: no data, no error.
    for path in ["/me/appointments", "/me/staff/day"] {
        let unlinked = s.get(&tech_b, path).await;
        assert_eq!(unlinked.status, StatusCode::OK, "{path} {}", unlinked.body);
        assert_eq!(unlinked.body, json!({"linked": false}), "{path}");
    }
    let b_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912100444'")
            .fetch_one(&s.h.owner)
            .await?;
    let thu = s
        .post(
            &s.owner,
            "/staff",
            json!({"full_name": "Phạm Thu", "position": "Kỹ thuật viên", "user_id": b_user}),
        )
        .await;
    assert_eq!(thu.status, StatusCode::CREATED, "{}", thu.body);
    let thu = id(&thu);

    let hang = s.customer("0905 222 001").await;
    let binh = s
        .post(
            &s.owner,
            "/customers",
            json!({"full_name": "Đỗ Văn Bình", "phone": "0905 222 002", "gender": "male",
                   "note": "Ghi chú riêng của quầy"}),
        )
        .await;
    assert_eq!(binh.status, StatusCode::CREATED, "{}", binh.body);
    let binh = id(&binh);
    // Shared visit: one line by Lan (A), two by Thu (B).
    let shared = s
        .post(
            &s.owner,
            "/visits",
            json!({
                "customer_id": hang, "note": "Khách đau vai gáy",
                "lines": [
                    {"service_id": s.service, "source": "one_off", "staff_id": s.lan, "qty": 1},
                    {"service_id": s.service, "source": "one_off", "staff_id": thu, "qty": 2}
                ],
                "payments": [{"method": "cash", "amount": PRICE * 3}]
            }),
        )
        .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{}", shared.body);
    let shared = id(&shared);
    // B alone, with a session note: never A's to read (item 3).
    let b_only = s
        .post(
            &s.owner,
            "/visits",
            json!({
                "customer_id": binh, "note": "Buổi riêng của Thu: đau lưng",
                "lines": [{"service_id": s.service, "source": "one_off", "staff_id": thu, "qty": 1}],
                "payments": [{"method": "cash", "amount": PRICE}]
            }),
        )
        .await;
    assert_eq!(b_only.status, StatusCode::CREATED, "{}", b_only.body);
    let b_only = id(&b_only);
    let lan_own = s.one_off_visit(binh, s.lan, 1).await;
    let voided = s.one_off_visit(binh, s.lan, 1).await;
    let void = s
        .post(
            &s.owner,
            &format!("/visits/{voided}/void"),
            json!({"reason": "Ghi nhầm"}),
        )
        .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);

    let starts = format!("{}T10:00:00+07:00", today() + Duration::days(2));
    let book_a = s
        .post(
            &s.manager,
            "/appointments",
            json!({
                "customer_id": binh, "service_ids": [s.service], "staff_id": s.lan,
                "starts_at": starts, "note": "Khách muốn phòng riêng"
            }),
        )
        .await;
    assert_eq!(book_a.status, StatusCode::CREATED, "{}", book_a.body);
    let book_b = s
        .post(
            &s.manager,
            "/appointments",
            json!({
                "guest": {"full_name": "Khách mới Yến", "phone": "0905 333 444"},
                "service_ids": [s.service], "staff_id": thu, "starts_at": starts
            }),
        )
        .await;
    assert_eq!(book_b.status, StatusCode::CREATED, "{}", book_b.body);
    Ok(Work {
        tech_b,
        thu,
        hang,
        binh,
        shared,
        b_only,
        lan_own,
        book_a: book_a.body,
    })
}

/// EM 2026-11-07: `GET /me/appointments` — technician A's own calendar holds the bookings
/// assigned to A and the visits A served (only A's lines of a shared visit, voided ones never);
/// never B's, never phones, notes, money or customer ids; unlinked → `linked: false`; no way to
/// ask for someone else; another spa sees nothing.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_sees_only_their_own_calendar(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let w = two_technicians_at_work(&s).await?;
    let week = format!(
        "/me/appointments?from={}&to={}",
        today(),
        today() + Duration::days(6)
    );

    // A (Lan).
    let a = s.get(&s.tech, &week).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert_eq!(
        a.headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(a.body["linked"], true);
    let entries = a.body["data"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let ids: Vec<&Value> = entries.iter().map(|e| &e["id"]).collect();
    assert_eq!(
        ids,
        [&json!(w.shared), &json!(w.lan_own), &w.book_a["id"]],
        "voided and B-only visits and B's booking excluded, earliest first: {}",
        a.body
    );
    let shared = &entries[0];
    assert_eq!(shared["kind"], "visit");
    assert_eq!(shared["walk_in"], true);
    assert_eq!(shared["customer_name"], "Nguyễn Thị Hằng");
    assert_eq!(shared["duration_min"], 60, "A's one line: 60 minutes");
    assert_eq!(
        shared["services"],
        json!([{"service_name": "Gội đầu thảo dược", "category_name": "Gội đầu dưỡng sinh",
                "from_package": false}]),
        "only A's own line of the shared visit"
    );
    let booked = &entries[2];
    assert_eq!(booked["kind"], "appointment");
    assert_eq!(booked["phase"], "waiting");
    assert_eq!(booked["customer_name"], "Đỗ Văn Bình");
    assert_eq!(booked["group_size"], Value::Null);
    // EM 2026-11-07 item 3: the session note of a visit A worked on, never the booking's note.
    assert_eq!(shared["note"], "Khách đau vai gáy");
    assert_eq!(entries[1]["note"], Value::Null, "A's own visit has no note");
    assert_eq!(
        booked["note"],
        Value::Null,
        "a booking's own note is not a session note"
    );
    // Allowlist: exactly these keys, nothing about money, phones, customer notes or ids.
    for e in &entries {
        assert_eq!(
            keys(e),
            [
                "business_date",
                "customer_name",
                "duration_min",
                "ends_at",
                "group_size",
                "id",
                "kind",
                "note",
                "phase",
                "services",
                "starts_at",
                "walk_in"
            ]
        );
    }
    let text = a.body.to_string();
    for leak in [
        w.thu.to_string(),
        w.b_only.to_string(),
        "Khách mới Yến".to_owned(),
        "Phạm Thu".to_owned(),
        "150000".to_owned(),
        "+84".to_owned(),
        "đau lưng".to_owned(),
        "phòng riêng".to_owned(),
        "Ghi chú riêng".to_owned(),
        w.hang.to_string(),
        w.binh.to_string(),
        s.lan.to_string(),
    ] {
        assert!(!text.contains(&leak), "leaked {leak}: {text}");
    }

    // B (Thu): the shared visit (B's 2× line), B's own visit, B's guest booking.
    let b = s.get(&w.tech_b, &week).await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.body);
    let b_entries = b.body["data"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(b_entries.len(), 3, "{}", b.body);
    assert_eq!(b_entries[0]["id"], json!(w.shared));
    assert_eq!(b_entries[0]["duration_min"], 60);
    assert_eq!(b_entries[1]["id"], json!(w.b_only));
    assert_eq!(
        b_entries[0]["note"], "Khách đau vai gáy",
        "B worked on the shared visit too"
    );
    assert_eq!(b_entries[1]["note"], "Buổi riêng của Thu: đau lưng");
    assert_eq!(b_entries[2]["customer_name"], "Khách mới Yến");
    let b_text = b.body.to_string();
    for leak in [s.lan.to_string(), w.lan_own.to_string(), "+84".to_owned()] {
        assert!(!b_text.contains(&leak), "leaked {leak}: {b_text}");
    }

    // No way to pick someone else; bad ranges → 422; another week is empty.
    let pick = s
        .get(&s.tech, &format!("/me/appointments?staff_id={}", w.thu))
        .await;
    assert_eq!(pick.status, StatusCode::BAD_REQUEST, "{}", pick.body);
    let backwards = format!(
        "/me/appointments?from={}&to={}",
        today(),
        today() - Duration::days(1)
    );
    let too_long = format!(
        "/me/appointments?from={}&to={}",
        today(),
        today() + Duration::days(14)
    );
    for path in [backwards, too_long] {
        assert_eq!(
            s.get(&s.tech, &path).await.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
    }
    let old = s.get(&s.tech, "/me/appointments?from=2021-01-04").await;
    assert_eq!(old.body["data"]["entries"], json!([]));

    // Another spa's owner (not linked there) sees nothing of ours; anonymous → 401.
    assert_eq!(s.get(&s.other, &week).await.body, json!({"linked": false}));
    let anon = call(&s.h.app, "GET", "/me/appointments", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);

    // Technicians still have no visit / customer / appointment access; managers keep theirs.
    for path in ["/visits", "/customers", "/appointments"] {
        assert_eq!(
            s.get(&s.tech, path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
        assert_eq!(
            s.get(&s.manager, path).await.status,
            StatusCode::OK,
            "{path}"
        );
    }
    Ok(())
}

/// EM 2026-11-07: `GET /me/staff/day` — one day of A's own month in detail: attendance, reviews,
/// bonuses, A's lines with their commission, packages A closed. Never B's work, phones or notes.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_sees_their_own_day_in_detail(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let w = two_technicians_at_work(&s).await?;
    s.pay(s.lan, 6_000_000, 1_000).await;
    assert_eq!(
        s.attend(&s.manager, s.lan, today(), "present").await.status,
        StatusCode::OK
    );
    let review = s
        .put(
            &s.manager,
            "/daily-reviews",
            json!({"entries": [{"staff_id": s.lan, "date": today(), "compliments": 2,
                                "complaints": 1, "note": "Khách khen tay nhẹ"}]}),
        )
        .await;
    assert_eq!(review.status, StatusCode::OK, "{}", review.body);
    for (staff, reason) in [(s.lan, "Ở lại dọn phòng"), (w.thu, "Thưởng của Thu")] {
        let r = s
            .post(
                &s.manager,
                "/adjustments",
                json!({"staff_id": staff, "kind": "bonus", "amount": 100_000, "reason": reason}),
            )
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    }
    let sale = s
        .post(
            &s.owner,
            "/sales",
            json!({
                "customer_id": w.hang, "closer_staff_id": s.lan,
                "lines": [{"package_plan_id": s.plan}],
                "payments": [{"method": "cash", "amount": PLAN_PRICE}]
            }),
        )
        .await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);

    let a = s
        .get(&s.tech, &format!("/me/staff/day?date={}", today()))
        .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert_eq!(
        a.headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let d = &a.body["data"];
    assert_eq!(d["date"], json!(today()));
    assert_eq!(d["commission_rate_bp"], 1_000);
    assert_eq!(d["attendance"]["status"], "present");
    assert_eq!(d["attendance"]["check_in"], "09:12");
    assert_eq!(d["review"]["compliments"], 2);
    assert_eq!(d["review"]["note"], "Khách khen tay nhẹ");
    let adjustments = d["adjustments"].as_array().cloned().unwrap_or_default();
    assert_eq!(adjustments.len(), 1, "only A's: {}", a.body);
    assert_eq!(adjustments[0]["reason"], "Ở lại dọn phòng");
    let visits = d["visits"].as_array().cloned().unwrap_or_default();
    let ids: Vec<&Value> = visits.iter().map(|v| &v["visit_id"]).collect();
    assert_eq!(ids, [&json!(w.shared), &json!(w.lan_own)], "{}", a.body);
    assert_eq!(
        visits[0]["lines"],
        json!([{"service_name": "Gội đầu thảo dược", "source": "one_off", "qty": 1,
                "value": PRICE, "commission": PRICE / 10}]),
        "only A's own line of the shared visit, 10 % of its value"
    );
    assert_eq!(
        keys(&visits[0]),
        ["check_in_at", "customer_name", "lines", "note", "visit_id"]
    );
    assert_eq!(
        visits[0]["note"], "Khách đau vai gáy",
        "A worked on it: the session note"
    );
    assert_eq!(visits[1]["note"], Value::Null);
    assert_eq!(
        d["upsell"],
        json!([{"kind": "package", "description": "Gói 10 buổi", "customer_name": "Nguyễn Thị Hằng",
                "qty": 1, "value": PLAN_PRICE, "commission": PLAN_PRICE / 10}])
    );
    let text = a.body.to_string();
    for leak in [
        w.thu.to_string(),
        w.b_only.to_string(),
        "Thưởng của Thu".to_owned(),
        "Phạm Thu".to_owned(),
        "+84".to_owned(),
        "đau lưng".to_owned(),
        "Ghi chú riêng".to_owned(),
        w.hang.to_string(),
        w.binh.to_string(),
    ] {
        assert!(!text.contains(&leak), "leaked {leak}: {text}");
    }

    // B sees B's lines of the same day (2× on the shared visit), B's bonus, no pay terms yet.
    let b = s.get(&w.tech_b, "/me/staff/day").await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.body);
    let bd = &b.body["data"];
    assert_eq!(bd["commission_rate_bp"], Value::Null);
    assert_eq!(bd["visits"][0]["lines"][0]["qty"], 2);
    assert_eq!(bd["visits"][0]["lines"][0]["commission"], Value::Null);
    assert_eq!(bd["visits"][1]["visit_id"], json!(w.b_only));
    assert_eq!(bd["visits"][1]["note"], "Buổi riêng của Thu: đau lưng");
    assert_eq!(bd["adjustments"][0]["reason"], "Thưởng của Thu");
    assert_eq!(bd["upsell"], json!([]));
    assert!(!b.body.to_string().contains("Ở lại dọn phòng"));

    // Another day is empty; no way to pick someone else; bad dates → 422.
    let before = s
        .get(
            &s.tech,
            &format!("/me/staff/day?date={}", today() - Duration::days(40)),
        )
        .await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.body);
    assert_eq!(before.body["data"]["visits"], json!([]));
    assert_eq!(before.body["data"]["attendance"], Value::Null);
    let pick = s
        .get(&s.tech, &format!("/me/staff/day?staff_id={}", w.thu))
        .await;
    assert_eq!(pick.status, StatusCode::BAD_REQUEST, "{}", pick.body);
    for bad in ["2026-13-01", "2026-02-30", "1999-01-01", "hom-nay"] {
        assert_eq!(
            s.get(&s.tech, &format!("/me/staff/day?date={bad}"))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }
    assert_eq!(
        s.get(&s.other, "/me/staff/day").await.body,
        json!({"linked": false})
    );
    let anon = call(&s.h.app, "GET", "/me/staff/day", None, None).await;
    assert_eq!(anon.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spa_cannot_reach_our_staff(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    assert_eq!(
        s.attend(&s.owner, s.lan, yesterday(), "present")
            .await
            .status,
        StatusCode::OK
    );

    let write = s.attend(&s.other, s.lan, yesterday(), "absent_k").await;
    assert_eq!(
        write.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        write.body
    );
    assert_eq!(write.body["errors"][0]["field"], "entries[0].staff_id");
    let adj = s
        .post(
            &s.other,
            "/adjustments",
            json!({
                "staff_id": s.lan, "kind": "penalty", "amount": 1_000, "reason": "x"
            }),
        )
        .await;
    assert_eq!(adj.status, StatusCode::UNPROCESSABLE_ENTITY);
    let eval = s
        .put(
            &s.other,
            "/evaluations",
            json!({
                "staff_id": s.lan, "month": ym(today()), "criteria": criteria("m1")
            }),
        )
        .await;
    assert_eq!(eval.status, StatusCode::UNPROCESSABLE_ENTITY);
    for path in [
        format!("/staff/{}", s.lan),
        format!("/payroll/payslips/{}", s.lan),
    ] {
        assert_eq!(
            s.get(&s.other, &path).await.status,
            StatusCode::NOT_FOUND,
            "{path}"
        );
    }
    let payroll = s.get(&s.other, "/payroll").await;
    assert_eq!(payroll.status, StatusCode::OK);
    assert!(!payroll.body.to_string().contains(&s.lan.to_string()));
    let sheet = s.get(&s.other, "/attendance").await;
    assert!(!sheet.body.to_string().contains(&s.lan.to_string()));

    // RLS, under app_rw: the other spa's transaction sees none of our rows.
    let app = app_pool(&s.h.owner).await?;
    let mut tx = TenantTx::begin(&app, TenantId::from_uuid(s.other_tenant)).await?;
    for sql in [
        "SELECT count(*) FROM app.attendance_day",
        "SELECT count(*) FROM app.staff_pay",
        "SELECT count(*) FROM app.payroll_period",
    ] {
        let n: i64 = sqlx::query_scalar(sql).fetch_one(tx.conn()).await?;
        assert_eq!(n, 0, "{sql}");
    }
    tx.commit().await?;
    Ok(())
}

/// A closed month is read-only for every record kind, in the API and in the database; payslips
/// are frozen; corrections are adjustments in an open month.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_closed_month_is_locked_and_corrected_by_adjustments(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    let last = end_of_last_month();
    let month = ym(last);
    assert_eq!(
        s.attend(&s.owner, s.lan, last, "present").await.status,
        StatusCode::OK
    );
    let ev = s
        .put(
            &s.manager,
            "/evaluations",
            json!({
                "staff_id": s.lan, "month": month, "criteria": criteria("m2"), "comment": "Ổn định"
            }),
        )
        .await;
    assert_eq!(ev.status, StatusCode::OK, "{}", ev.body);
    assert_eq!(
        (ev.body["total_points"].clone(), ev.body["result"].clone()),
        (json!(80), json!("m2"))
    );
    let bonus = s.post(&s.manager, "/adjustments", json!({
        "staff_id": s.lan, "kind": "bonus", "amount": 200_000, "reason": "Khách khen", "occurred_on": last
    })).await;
    assert_eq!(bonus.status, StatusCode::CREATED, "{}", bonus.body);

    let current = s
        .post(&s.owner, "/payroll/close", json!({"month": ym(today())}))
        .await;
    assert_eq!(current.body["code"], "payroll.month_not_ended");
    let incomplete = s
        .post(&s.owner, "/payroll/close", json!({"month": month}))
        .await;
    assert_eq!(
        (incomplete.status, incomplete.body["code"].clone()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!("payroll.unrecorded_days")
        )
    );
    assert_eq!(
        s.post(&s.manager, "/payroll/close", json!({"month": month}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let key = Uuid::now_v7().to_string();
    let closed = s
        .post_key(
            &s.owner,
            "/payroll/close",
            json!({"month": month, "allow_incomplete": true}),
            &key,
        )
        .await;
    assert_eq!(closed.status, StatusCode::OK, "{}", closed.body);
    assert_eq!(closed.body["closed"], true);
    let frozen = slip(&closed.body, s.lan).clone();
    assert_eq!(frozen["bonuses"], 200_000);
    assert_eq!(frozen["evaluation_level"], "m2");
    let replay = s
        .post_key(
            &s.owner,
            "/payroll/close",
            json!({"month": month, "allow_incomplete": true}),
            &key,
        )
        .await;
    assert_eq!(
        (replay.status, replay.body.clone()),
        (StatusCode::OK, closed.body.clone()),
        "replay"
    );
    let again = s
        .post(
            &s.owner,
            "/payroll/close",
            json!({"month": month, "allow_incomplete": true}),
        )
        .await;
    assert_eq!(
        (again.status, again.body["code"].clone()),
        (StatusCode::CONFLICT, json!("payroll.period_closed"))
    );

    // Every write into the month is refused.
    let locked = [
        s.attend(&s.owner, s.lan, last, "absent_k").await,
        s.put(&s.owner, "/daily-reviews", json!({"entries": [{"staff_id": s.lan, "date": last, "complaints": 1}]})).await,
        s.put(&s.owner, "/evaluations", json!({"staff_id": s.lan, "month": month, "criteria": criteria("m1")})).await,
        s.post(&s.owner, "/adjustments", json!({
            "staff_id": s.lan, "kind": "penalty", "amount": 10_000, "reason": "x", "occurred_on": last
        })).await,
    ];
    for r in &locked {
        assert_eq!(
            (r.status, r.body["code"].clone()),
            (StatusCode::CONFLICT, json!("payroll.period_closed")),
            "{}",
            r.body
        );
    }

    // An entry of a closed month can no longer be removed or edited either.
    let bonus_path = format!("/adjustments/{}", id(&bonus));
    let removed = call(&s.h.app, "DELETE", &bonus_path, Some(&s.owner), None).await;
    assert_eq!(
        (removed.status, removed.body["code"].clone()),
        (StatusCode::CONFLICT, json!("payroll.period_closed"))
    );
    let edited = call_with_headers(
        &s.h.app,
        "PUT",
        &bonus_path,
        Some(&s.owner),
        Some(json!({"staff_id": s.lan, "kind": "bonus", "amount": 1_000, "reason": "x"})),
        &[("Idempotency-Key", &Uuid::now_v7().to_string())],
    )
    .await;
    assert_eq!(
        edited.body["code"], "payroll.period_closed",
        "{}",
        edited.body
    );
    let voided =
        sqlx::query("UPDATE app.staff_adjustment SET voided_at = now(), voided_by = created_by")
            .execute(&s.h.owner)
            .await;
    assert!(
        voided.is_err(),
        "the database keeps a closed month's entries too"
    );

    // The frozen payslip does not follow later pay changes.
    s.pay(s.lan, 9_000_000, 1000).await;
    let after = s.get(&s.owner, &format!("/payroll?month={month}")).await;
    assert_eq!(slip(&after.body, s.lan), &frozen);
    let one = s
        .get(
            &s.owner,
            &format!("/payroll/payslips/{}?month={month}", s.lan),
        )
        .await;
    assert_eq!(
        (
            one.body["closed"].clone(),
            one.body["payslip"]["net_pay"].clone()
        ),
        (json!(true), frozen["net_pay"].clone())
    );

    // Correction: an adjustment in an open month that names the closed one.
    let fix = s.post(&s.owner, "/adjustments", json!({
        "staff_id": s.lan, "kind": "bonus", "amount": 50_000, "reason": "Bù thưởng tháng trước",
        "corrects_month": month
    })).await;
    assert_eq!(fix.status, StatusCode::CREATED, "{}", fix.body);
    assert_eq!(fix.body["corrects_month"], month);
    let open_fix = s
        .post(
            &s.owner,
            "/adjustments",
            json!({
                "staff_id": s.lan, "kind": "bonus", "amount": 50_000, "reason": "x",
                "corrects_month": ym(today())
            }),
        )
        .await;
    assert_eq!(open_fix.status, StatusCode::UNPROCESSABLE_ENTITY);
    let now = s.get(&s.owner, "/payroll").await;
    assert_eq!(slip(&now.body, s.lan)["bonuses"], 50_000);

    // The database refuses it too, even under app_rw with the right tenant.
    let app = app_pool(&s.h.owner).await?;
    for sql in [
        "UPDATE app.attendance_day SET note = 'x'",
        "DELETE FROM app.attendance_day",
        "UPDATE app.payroll_period SET status = 'open', closed_at = NULL, closed_by = NULL",
        "DELETE FROM app.payslip",
        "UPDATE app.staff_adjustment SET amount = 1",
    ] {
        let mut tx = TenantTx::begin(&app, TenantId::from_uuid(s.tenant)).await?;
        let err = sqlx::query(sql).execute(tx.conn()).await;
        assert!(err.is_err(), "{sql} was allowed");
    }

    // Every change above left an audit row with the acting user.
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT action FROM app.audit_log WHERE tenant_id = $1 AND actor_user_id IS NOT NULL ORDER BY 1",
    )
    .bind(s.tenant)
    .fetch_all(&s.h.owner)
    .await?;
    for a in [
        "attendance.record",
        "evaluation.record",
        "payroll.close",
        "staff.adjustment",
        "staff.create",
        "staff.pay_update",
    ] {
        assert!(actions.iter().any(|x| x == a), "no audit {a}: {actions:?}");
    }
    Ok(())
}

/// Commission = rate × the line values of completed visits the person performed this month
/// (+ rate × package sales they closed), and the base reconciles with the visits table.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn commission_reconciles_with_visits(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    s.pay(s.mai, 5_000_000, 1000).await;
    let hang = s.customer("0905 222 001").await;
    s.one_off_visit(hang, s.lan, 2).await;
    s.one_off_visit(hang, s.mai, 1).await;
    let wrong = s.one_off_visit(hang, s.lan, 1).await;
    let void = s
        .post(
            &s.owner,
            &format!("/visits/{wrong}/void"),
            json!({"reason": "Ghi nhầm"}),
        )
        .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);
    let sale = s.post(&s.owner, "/sales", json!({
        "customer_id": hang, "closer_staff_id": s.lan, "lines": [{"package_plan_id": s.plan}],
        "payments": [{"method": "cash", "amount": PLAN_PRICE}]
    })).await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);
    let pkg = s.post(&s.owner, "/visits", json!({
        "customer_id": hang, "lines": [{"service_id": s.service, "source": "package", "staff_id": s.mai}]
    })).await;
    assert_eq!(pkg.status, StatusCode::CREATED, "{}", pkg.body);

    // The attendance sheet knows who served customers today (the leave warning).
    let sheet = s.get(&s.owner, "/attendance").await;
    for staff in [s.lan, s.mai] {
        let days = sheet.body["staff"]
            .as_array()
            .and_then(|a| a.iter().find(|x| x["staff_id"] == json!(staff)))
            .map(|x| x["visit_days"].clone());
        assert_eq!(days, Some(json!([today()])), "{staff}");
    }

    let payroll = s.get(&s.owner, "/payroll").await;
    assert_eq!(payroll.status, StatusCode::OK, "{}", payroll.body);
    let lan = slip(&payroll.body, s.lan);
    assert_eq!(
        lan["service_commission_base"],
        2 * PRICE,
        "voided visit excluded"
    );
    assert_eq!(lan["service_commission"], 15_000);
    assert_eq!(
        (
            lan["closing_commission_base"].clone(),
            lan["closing_commission"].clone()
        ),
        (json!(PLAN_PRICE), json!(60_000))
    );
    let mai = slip(&payroll.body, s.mai);
    // A package session is worth price ÷ paid sessions (1.200.000 ÷ 10).
    assert_eq!(mai["service_commission_base"], PRICE + 120_000);
    assert_eq!(mai["service_commission"], 27_000);
    assert_eq!(mai["commission_lines"].as_array().map(Vec::len), Some(2));

    // Independent reconciliation against the visit tables.
    let start = today().with_day(1).unwrap_or_default();
    for staff in [s.lan, s.mai] {
        let base: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(vl.line_value), 0)::bigint FROM app.visit_line vl
             JOIN app.visit v ON v.tenant_id = vl.tenant_id AND v.id = vl.visit_id
             WHERE vl.tenant_id = $1 AND vl.staff_id = $2 AND v.status = 'completed'
               AND v.business_date >= $3",
        )
        .bind(s.tenant)
        .bind(staff)
        .bind(start)
        .fetch_one(&s.h.owner)
        .await?;
        assert_eq!(slip(&payroll.body, staff)["service_commission_base"], base);
    }
    assert_eq!(
        payroll.body["totals"]["commission"],
        15_000 + 60_000 + 27_000
    );

    // Rules: gift sessions can count, closing commission can be switched off.
    let rules = s
        .put(
            &s.owner,
            "/payroll/rules",
            json!({
                "commission_on_bonus_sessions": true, "closing_commission": false
            }),
        )
        .await;
    assert_eq!(rules.status, StatusCode::OK, "{}", rules.body);
    let p = s.get(&s.owner, "/payroll").await;
    assert_eq!(slip(&p.body, s.lan)["closing_commission"], 0);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn rules_and_attendance_drive_deductions(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    let bad = s
        .put(
            &s.owner,
            "/payroll/rules",
            json!({"late_penalty_amount": -5}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    let unknown = s
        .put(
            &s.owner,
            "/payroll/rules",
            json!({"pay_everyone_double": true}),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
    let ok = s.put(&s.owner, "/payroll/rules", json!({
        "free_leave_codes": ["leave_p", "leave_dx"], "late_grace_minutes": 10, "late_penalty_amount": 20_000
    })).await;
    assert_eq!(
        (ok.status, ok.body["version"].clone()),
        (StatusCode::OK, json!(1))
    );
    assert_eq!(
        s.put(&s.manager, "/payroll/rules", json!({})).await.status,
        StatusCode::FORBIDDEN
    );

    let last = end_of_last_month();
    assert_eq!(
        s.attend(&s.owner, s.lan, last, "absent_k").await.status,
        StatusCode::OK
    );
    assert_eq!(
        s.attend(&s.owner, s.lan, last - Duration::days(1), "present")
            .await
            .status,
        StatusCode::OK
    );
    let p = s
        .get(&s.owner, &format!("/payroll?month={}", ym(last)))
        .await;
    let lan = slip(&p.body, s.lan);
    assert_eq!(lan["deducted_leave_days"], 1, "K is no longer free");
    assert_eq!(
        (lan["late_penalised"].clone(), lan["late_deduction"].clone()),
        (json!(1), json!(20_000)),
        "12 min > 10"
    );
    assert!(
        lan["flags"]
            .as_array()
            .is_some_and(|f| f.contains(&json!("unrecorded_days")))
    );

    let future = s
        .attend(&s.owner, s.lan, today() + Duration::days(1), "present")
        .await;
    assert_eq!(future.status, StatusCode::UNPROCESSABLE_ENTITY);
    let cleared = s
        .put(
            &s.owner,
            "/attendance",
            json!({"entries": [{"staff_id": s.lan, "date": last, "status": null}]}),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    let sheet = s
        .get(&s.owner, &format!("/attendance?month={}", ym(last)))
        .await;
    let days = sheet.body["staff"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["staff_id"] == json!(s.lan)))
        .map(|x| x["days"].as_array().map(Vec::len));
    assert_eq!(days, Some(Some(1)));
    let fail_no_plan = s
        .put(
            &s.owner,
            "/evaluations",
            json!({
                "staff_id": s.lan, "month": ym(last), "criteria": criteria("m3")
            }),
        )
        .await;
    assert_eq!(fail_no_plan.body["errors"][0]["field"], "coaching_plan");
    Ok(())
}

// ------------------------------------------------------------------- decision C (2026-11-07)

const SECOND_MANAGER_PHONE: &str = "0912100444";

/// A staff profile linked to the login with this phone.
async fn linked_profile(s: &Spa, phone: &str, name: &str) -> Setup<Uuid> {
    let user: Uuid = sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
        .bind(format!("+84{}", &phone[1..]))
        .fetch_one(&s.h.owner)
        .await?;
    let r = call(
        &s.h.app,
        "POST",
        "/staff",
        Some(&s.owner),
        Some(json!({"full_name": name, "position": "Quản lý", "user_id": user})),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    Ok(id(&r))
}

/// Decision C, every write route: a manager records attendance, praise/complaints,
/// bonuses/penalties and evaluations for technicians only, never for themselves or another
/// manager (`403 staff.record_owner_only`, nothing written); the owner records them for anyone.
/// The month views say which rows the caller may write. Mutation-checked: making
/// `RecordScope::may_record` return true turns this red.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_records_for_a_manager(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    add_member(
        &s.h.owner,
        "vpyd",
        SECOND_MANAGER_PHONE,
        "Quản lý Thu",
        SystemRole::Manager,
    )
    .await?;
    let hoa = linked_profile(&s, MANAGER_PHONE, "Quản lý Hoa").await?;
    let thu = linked_profile(&s, SECOND_MANAGER_PHONE, "Quản lý Thu").await?;
    let day = yesterday();
    let month = ym(day);
    let owner_only = |r: &Reply| {
        assert_eq!(
            (r.status, r.body["code"].clone()),
            (StatusCode::FORBIDDEN, json!("staff.record_owner_only")),
            "{}",
            r.body
        );
    };

    for supervisor in [hoa, thu] {
        owner_only(&s.attend(&s.manager, supervisor, day, "present").await);
        owner_only(
            &s.put(
                &s.manager,
                "/daily-reviews",
                json!({"entries": [
                    {"staff_id": supervisor, "date": day, "compliments": 1}
                ]}),
            )
            .await,
        );
        owner_only(
            &s.put(
                &s.manager,
                "/evaluations",
                json!({
                    "staff_id": supervisor, "month": month, "criteria": criteria("m1")
                }),
            )
            .await,
        );
        owner_only(
            &s.post(&s.manager, "/adjustments", json!({
                "staff_id": supervisor, "kind": "bonus", "amount": 100_000, "reason": "Tự thưởng"
            }))
            .await,
        );
    }
    // A batch with one forbidden row writes nothing at all.
    owner_only(
        &s.put(
            &s.manager,
            "/attendance",
            json!({"entries": [
                {"staff_id": s.lan, "date": day, "status": "present"},
                {"staff_id": hoa, "date": day, "status": "present"}
            ]}),
        )
        .await,
    );
    let written: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM app.attendance_day) + (SELECT count(*) FROM app.staff_daily_review)
              + (SELECT count(*) FROM app.staff_evaluation) + (SELECT count(*) FROM app.staff_adjustment)",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(written, 0, "every refusal wrote nothing");

    // Technicians: the manager records everything.
    assert_eq!(
        s.attend(&s.manager, s.lan, day, "present").await.status,
        StatusCode::OK
    );
    let review = s
        .put(
            &s.manager,
            "/daily-reviews",
            json!({"entries": [
                {"staff_id": s.lan, "date": day, "compliments": 1}
            ]}),
        )
        .await;
    assert_eq!(review.status, StatusCode::OK, "{}", review.body);
    let eval = s
        .put(
            &s.manager,
            "/evaluations",
            json!({
                "staff_id": s.mai, "month": month, "criteria": criteria("m1")
            }),
        )
        .await;
    assert_eq!(eval.status, StatusCode::OK, "{}", eval.body);

    // The owner records for the managers.
    assert_eq!(
        s.attend(&s.owner, hoa, day, "present").await.status,
        StatusCode::OK
    );
    let review = s
        .put(
            &s.owner,
            "/daily-reviews",
            json!({"entries": [
                {"staff_id": thu, "date": day, "complaints": 1}
            ]}),
        )
        .await;
    assert_eq!(review.status, StatusCode::OK, "{}", review.body);
    let eval = s
        .put(
            &s.owner,
            "/evaluations",
            json!({
                "staff_id": hoa, "month": month, "criteria": criteria("m1")
            }),
        )
        .await;
    assert_eq!(eval.status, StatusCode::OK, "{}", eval.body);
    let owners_bonus = s
        .post(
            &s.owner,
            "/adjustments",
            json!({
                "staff_id": hoa, "kind": "bonus", "amount": 300_000, "reason": "Quản lý tốt"
            }),
        )
        .await;
    assert_eq!(
        owners_bonus.status,
        StatusCode::CREATED,
        "{}",
        owners_bonus.body
    );

    // Removing or editing an entry follows the same rule as recording it.
    let theirs = format!("/adjustments/{}", id(&owners_bonus));
    owner_only(&call(&s.h.app, "DELETE", &theirs, Some(&s.manager), None).await);
    let app = &s.h.app;
    let edit = |session: &str, path: &str, staff: Uuid, amount: i64| {
        let (session, path) = (session.to_owned(), path.to_owned());
        async move {
            let key = Uuid::now_v7().to_string();
            call_with_headers(
                app,
                "PUT",
                &path,
                Some(&session),
                Some(json!({"staff_id": staff, "kind": "penalty", "amount": amount, "reason": "Đi muộn"})),
                &[("Idempotency-Key", &key)],
            )
            .await
        }
    };
    owner_only(&edit(&s.manager, &theirs, s.lan, 50_000).await);

    // The views tell the UI which rows the caller may write.
    let flags = |body: &Value, key: &str| -> Vec<(Uuid, bool)> {
        body["staff"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| {
                        (
                            x["staff_id"]
                                .as_str()
                                .and_then(|v| Uuid::parse_str(v).ok())
                                .unwrap_or_default(),
                            x[key].as_bool().unwrap_or(true),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    for path in [
        format!("/attendance?month={month}"),
        format!("/daily-reviews?month={month}"),
        format!("/evaluations?month={month}"),
    ] {
        let as_manager = flags(&s.get(&s.manager, &path).await.body, "can_record");
        for (staff, can) in &as_manager {
            assert_eq!(*can, *staff != hoa && *staff != thu, "{path} {staff}");
        }
        assert_eq!(as_manager.len(), 4, "{path}");
        let as_owner = flags(&s.get(&s.owner, &path).await.body, "can_record");
        assert!(as_owner.iter().all(|(_, can)| *can), "{path}");
    }
    let log = s
        .get(&s.manager, &format!("/adjustments?month={}", ym(today())))
        .await;
    assert_eq!(log.body["items"][0]["can_edit"], false, "{}", log.body);

    // A manager's own entry for a technician: edited (void + new) and removed once.
    let mine = s
        .post(
            &s.manager,
            "/adjustments",
            json!({
                "staff_id": s.lan, "kind": "penalty", "amount": 20_000, "reason": "Đi muộn"
            }),
        )
        .await;
    assert_eq!(mine.status, StatusCode::CREATED, "{}", mine.body);
    let path = format!("/adjustments/{}", id(&mine));
    // Moving it onto a manager is the same as recording it for them.
    owner_only(&edit(&s.manager, &path, hoa, 20_000).await);
    let edited = edit(&s.manager, &path, s.lan, 30_000).await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_ne!(id(&edited), id(&mine), "an edit is a new entry");
    assert_eq!(edited.body["amount"], 30_000);
    let stale = edit(&s.manager, &path, s.lan, 40_000).await;
    assert_eq!(
        (stale.status, stale.body["code"].clone()),
        (StatusCode::CONFLICT, json!("adjustment.voided"))
    );
    let new_path = format!("/adjustments/{}", id(&edited));
    let removed = call(&s.h.app, "DELETE", &new_path, Some(&s.manager), None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.body);
    let again = call(&s.h.app, "DELETE", &new_path, Some(&s.manager), None).await;
    assert_eq!(again.body["code"], "adjustment.voided");
    let missing = call(
        &s.h.app,
        "DELETE",
        &format!("/adjustments/{}", Uuid::now_v7()),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let tech = call(&s.h.app, "DELETE", &theirs, Some(&s.tech), None).await;
    assert_eq!(tech.status, StatusCode::FORBIDDEN);
    // The other spa cannot see, let alone remove, our entries.
    let other = call(&s.h.app, "DELETE", &theirs, Some(&s.other), None).await;
    assert_eq!(other.status, StatusCode::NOT_FOUND);

    // Removed entries leave the log and the payslip; the row stays for the audit trail.
    let log = s
        .get(&s.owner, &format!("/adjustments?month={}", ym(today())))
        .await;
    let amounts: Vec<i64> = log.body["items"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["amount"].as_i64()).collect())
        .unwrap_or_default();
    assert_eq!(amounts, [300_000]);
    assert_eq!(log.body["items"][0]["can_edit"], true);
    s.pay(s.lan, 5_000_000, 500).await;
    let payroll = s.get(&s.owner, "/payroll").await;
    assert_eq!(slip(&payroll.body, s.lan)["penalties"], 0);
    let kept: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.staff_adjustment WHERE staff_id = $1 AND voided_at IS NOT NULL",
    )
    .bind(s.lan)
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(kept, 2, "voided, not deleted");
    // The database refuses any other change, even from the schema owner.
    let tampered = sqlx::query("UPDATE app.staff_adjustment SET amount = 1 WHERE staff_id = $1")
        .bind(s.lan)
        .execute(&s.h.owner)
        .await;
    assert!(tampered.is_err(), "only voiding is allowed");
    let deleted = sqlx::query("DELETE FROM app.staff_adjustment")
        .execute(&s.h.owner)
        .await;
    assert!(deleted.is_err(), "never deleted");
    Ok(())
}

// ------------------------------------------------------------------------- roster (0025)

impl Spa {
    /// `method` with `If-Match: "<version>"` (roster writes are optimistic).
    async fn versioned(
        &self,
        session: &str,
        method: &str,
        path: &str,
        body: Value,
        version: &Value,
    ) -> Reply {
        let tag = format!("\"{}\"", version.as_i64().unwrap_or_default());
        call_with_headers(
            &self.h.app,
            method,
            path,
            Some(session),
            Some(body),
            &[("If-Match", tag.as_str())],
        )
        .await
    }

    async fn profile(&self, staff: Uuid) -> Value {
        let r = self.get(&self.owner, &format!("/staff/{staff}")).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body
    }

    /// Ends `staff`'s employment as the owner.
    async fn end(&self, staff: Uuid, on: NaiveDate, reason: &str) -> Reply {
        let p = self.profile(staff).await;
        self.versioned(
            &self.owner,
            "POST",
            &format!("/staff/{staff}/end"),
            json!({"ended_on": on, "reason": reason}),
            &p["version"],
        )
        .await
    }

    async fn rehire(&self, staff: Uuid, on: NaiveDate) -> Reply {
        let p = self.profile(staff).await;
        self.versioned(
            &self.owner,
            "POST",
            &format!("/staff/{staff}/rehire"),
            json!({"started_on": on}),
            &p["version"],
        )
        .await
    }
}

fn names(list: &Value) -> Vec<String> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p["full_name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// EM 2026-11-07 item 9 (spec §13, ADR-0013): hire → work → end employment (soft) → rehire.
/// Ending needs a reason, refuses a future date, a date before recorded work and open bookings;
/// the person leaves every picker and is refused as KTV, but this month's attendance, payroll and
/// visits keep them; rehiring opens a second period and tenure sums both. Mutation-checked:
/// skipping `repo::worked_after` or `bookings_after` in `profile::end` turns this red.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_roster_hires_ends_and_rehires_without_losing_history(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let joined = today() - Duration::days(40);
    let hired = s
        .post(
            &s.owner,
            "/staff",
            json!({"full_name": "Nguyễn Thu", "position": "Kỹ thuật viên", "joined_on": joined,
                   "note": "Có chứng chỉ hành nghề"}),
        )
        .await;
    assert_eq!(hired.status, StatusCode::CREATED, "{}", hired.body);
    let thu = id(&hired);
    assert_eq!(hired.body["status"], "active");
    assert_eq!(hired.body["periods"].as_array().map(Vec::len), Some(1));
    assert_eq!(hired.body["tenure_days"], 41);
    assert_eq!(hired.body["tenure_months"], 1);
    assert_eq!(hired.body["details"]["note"], "Có chứng chỉ hành nghề");
    let picker = s.get(&s.manager, "/staff").await;
    assert!(names(&picker.body).contains(&"Nguyễn Thu".to_owned()));

    // Thu works: attendance yesterday, a visit today, a booking tomorrow.
    assert_eq!(
        s.attend(&s.owner, thu, yesterday(), "present").await.status,
        StatusCode::OK
    );
    let customer = s.customer("0905123987").await;
    s.one_off_visit(customer, thu, 1).await;
    let tomorrow = today() + Duration::days(1);
    let booking = s
        .post(
            &s.owner,
            "/appointments",
            json!({"customer_id": customer, "service_ids": [s.service], "staff_id": thu,
                   "starts_at": format!("{tomorrow}T10:00:00+07:00")}),
        )
        .await;
    assert_eq!(booking.status, StatusCode::CREATED, "{}", booking.body);

    // Only the owner ends employment; the manager and the technician are refused.
    let p = s.profile(thu).await;
    for who in [&s.manager, &s.tech] {
        let r = s
            .versioned(
                who,
                "POST",
                &format!("/staff/{thu}/end"),
                json!({"ended_on": today(), "reason": "x"}),
                &p["version"],
            )
            .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    }
    let stale = s
        .versioned(
            &s.owner,
            "POST",
            &format!("/staff/{thu}/end"),
            json!({"ended_on": today(), "reason": "Chuyển công tác"}),
            &json!(99),
        )
        .await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    let blank = s.end(thu, today(), "  ").await;
    assert_eq!(blank.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(blank.body["errors"][0]["field"], "reason");
    let future = s.end(thu, tomorrow, "Chuyển công tác").await;
    assert_eq!(
        future.body["errors"][0]["code"], "in_future",
        "{}",
        future.body
    );
    let before_work = s.end(thu, yesterday(), "Chuyển công tác").await;
    assert_eq!(
        before_work.body["errors"][0]["code"], "has_later_records",
        "a visit today: {}",
        before_work.body
    );
    let booked = s.end(thu, today(), "Chuyển công tác").await;
    assert_eq!(booked.status, StatusCode::CONFLICT);
    assert_eq!(booked.body["code"], "staff.has_upcoming_bookings");
    sqlx::query("UPDATE app.appointment SET staff_id = NULL WHERE staff_id = $1")
        .bind(thu)
        .execute(&s.h.owner)
        .await?;

    let ended = s.end(thu, today(), "Chuyển công tác").await;
    assert_eq!(ended.status, StatusCode::OK, "{}", ended.body);
    assert_eq!(ended.body["status"], "left");
    assert_eq!(ended.body["left_on"], json!(today()));
    assert_eq!(ended.body["periods"][0]["end_reason"], "Chuyển công tác");
    assert_eq!(
        s.end(thu, today(), "again").await.body["code"],
        "staff.already_left"
    );

    // Out of every picker, refused as KTV…
    let picker = s.get(&s.manager, "/staff").await;
    assert!(!names(&picker.body).contains(&"Nguyễn Thu".to_owned()));
    let left = s.get(&s.manager, "/staff?status=left").await;
    assert_eq!(names(&left.body), ["Nguyễn Thu"]);
    let refused = s
        .post(
            &s.owner,
            "/visits",
            json!({
                "customer_id": customer,
                "lines": [{"service_id": s.service, "source": "one_off", "staff_id": thu}],
                "payments": [{"method": "cash", "amount": PRICE}]
            }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refused.body["code"], "staff.inactive");
    // …but this month's books keep them: attendance, payroll (the visit's commission), visits.
    let sheet = s.get(&s.owner, "/attendance").await;
    assert!(sheet.body.to_string().contains(&thu.to_string()));
    s.pay(thu, 5_000_000, 500).await;
    let payroll = s.get(&s.owner, "/payroll").await;
    assert_eq!(slip(&payroll.body, thu)["service_commission_base"], PRICE);

    // The roster: manager sees dates and tenure, never the note or the reason.
    let roster = s.get(&s.manager, "/staff/profiles?status=left").await;
    assert_eq!(roster.status, StatusCode::OK, "{}", roster.body);
    assert_eq!(names(&roster.body), ["Nguyễn Thu"]);
    let text = roster.body.to_string();
    for leak in ["Chuyển công tác", "chứng chỉ", "\"details\"", "end_reason"] {
        assert!(!text.contains(leak), "manager got {leak}: {text}");
    }
    // Exact allowlist for a manager (list and one profile): no pay, phone, note, login id or
    // reason — only what pickers, the daily book and "làm bao lâu" need.
    let one = s.get(&s.manager, &format!("/staff/{thu}")).await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.body);
    for entry in [&roster.body[0], &one.body] {
        assert_eq!(
            keys(entry),
            [
                "first_joined_on",
                "full_name",
                "id",
                "joined_on",
                "left_on",
                "login_linked",
                "pay_visible",
                "periods",
                "position",
                "shift_start",
                "status",
                "tenure_days",
                "tenure_months",
                "version"
            ],
            "{entry}"
        );
        assert_eq!(entry["pay_visible"], false);
        for period in entry["periods"].as_array().into_iter().flatten() {
            assert_eq!(keys(period), ["ended_on", "started_on"], "{period}");
        }
    }
    // The owner gets the details; technicians get nothing from the roster at all.
    let owner_view = s.profile(thu).await;
    assert_eq!(owner_view["details"]["note"], "Có chứng chỉ hành nghề");
    assert_eq!(owner_view["periods"][0]["end_reason"], "Chuyển công tác");
    for path in [
        "/staff/profiles".to_owned(),
        format!("/staff/{thu}"),
        format!("/staff/{thu}/history"),
    ] {
        assert_eq!(
            s.get(&s.tech, &path).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    let tech_link = s
        .versioned(
            &s.tech,
            "PUT",
            &format!("/staff/{thu}/account"),
            json!({"user_id": null}),
            &owner_view["version"],
        )
        .await;
    assert_eq!(tech_link.status, StatusCode::FORBIDDEN);
    let manager_pay = s
        .put(
            &s.manager,
            &format!("/staff/{thu}/pay"),
            json!({"base_salary": 1, "commission_rate_bp": 0}),
        )
        .await;
    assert_eq!(manager_pay.status, StatusCode::FORBIDDEN);
    let working = s.get(&s.owner, "/staff/profiles").await;
    assert_eq!(
        names(&working.body).len(),
        3,
        "default all: Lan, Mai, Thu: {}",
        working.body
    );

    // Rehire: after the end only; a second period; tenure sums both.
    let same_day = s.rehire(thu, today()).await;
    assert_eq!(same_day.body["errors"][0]["code"], "before_last_end");
    let back = s.rehire(thu, tomorrow).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.body);
    assert_eq!(back.body["status"], "active");
    assert_eq!(back.body["joined_on"], json!(tomorrow));
    assert_eq!(back.body["first_joined_on"], json!(joined));
    assert_eq!(back.body["periods"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        back.body["tenure_days"], 41,
        "the new period starts tomorrow"
    );
    assert_eq!(
        s.rehire(thu, tomorrow).await.body["code"],
        "staff.already_active"
    );
    // The ended period can no longer change start: editing joined_on moves the new one only.
    let p = s.profile(thu).await;
    let overlap = s
        .versioned(
            &s.owner,
            "PUT",
            &format!("/staff/{thu}"),
            json!({"full_name": "Nguyễn Thu", "position": "Quản lý", "joined_on": today()}),
            &p["version"],
        )
        .await;
    assert_eq!(overlap.body["errors"][0]["code"], "overlaps_previous");

    // The history tells who did what (owner only).
    let history = s.get(&s.owner, &format!("/staff/{thu}/history")).await;
    let actions: Vec<&str> = history.body.as_array().map_or_else(Vec::new, |a| {
        a.iter().filter_map(|h| h["action"].as_str()).collect()
    });
    assert_eq!(
        actions,
        [
            "staff.rehire",
            "staff.pay_update",
            "staff.end",
            "staff.create"
        ]
    );
    assert_eq!(history.body[2]["reason"], "Chuyển công tác");
    assert_eq!(
        s.get(&s.manager, &format!("/staff/{thu}/history"))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // The database keeps history: an ended period never changes and nothing is deleted.
    let app = app_pool(&s.h.owner).await?;
    let mut tx = TenantTx::begin(&app, TenantId::from_uuid(s.tenant)).await?;
    let tampered = sqlx::query(
        "UPDATE app.staff_employment SET end_reason = 'x' WHERE staff_id = $1 AND ended_on IS NOT NULL",
    )
    .bind(thu)
    .execute(tx.conn())
    .await;
    assert!(tampered.is_err(), "an ended period is final");
    drop(tx);
    let mut tx = TenantTx::begin(&app, TenantId::from_uuid(s.tenant)).await?;
    let deleted = sqlx::query("DELETE FROM app.staff_employment")
        .execute(tx.conn())
        .await;
    assert!(deleted.is_err(), "app_rw cannot delete periods");
    Ok(())
}

/// The permissions `GET /me` reports for a session.
async fn permissions(s: &Spa, session: &str) -> Vec<String> {
    let me = s.get(session, "/me").await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    me.body["permissions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn next_month_day() -> NaiveDate {
    let t = today();
    t - Duration::days(i64::from(t.day()) - 1) + Duration::days(40)
}

/// EM 2026-11-07 Q-S6 (ADR-0014): a leaver's login still signs in, but from the next request it
/// holds only `staff.read_own_past`: their own month and day views, for months they worked in —
/// nothing else (no customers, Quầy, appointments, catalog, "Khách của tôi"), no month after
/// they left. Rehiring gives the role back. The owner may lock the login outright when ending
/// employment. Mutation-checked: making `effective_permissions` ignore `former_staff` turns
/// this red.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_leaver_keeps_only_their_own_past_pay(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.pay(s.lan, 5_000_000, 500).await;
    assert_eq!(
        s.attend(&s.owner, s.lan, today(), "present").await.status,
        StatusCode::OK
    );
    assert_eq!(
        s.get(&s.tech, "/me/appointments").await.status,
        StatusCode::OK
    );
    let ended = s.end(s.lan, today(), "Nghỉ sinh").await;
    assert_eq!(ended.status, StatusCode::OK, "{}", ended.body);
    assert_eq!(ended.body["login_linked"], true);

    // The live session and a fresh login both reach only their own past pay.
    let fresh = session_for(&s.h, TECH_PHONE).await;
    for session in [&s.tech, &fresh] {
        assert_eq!(permissions(&s, session).await, ["staff.read_own_past"]);
        let month = s.get(session, "/me/staff").await;
        assert_eq!(month.status, StatusCode::OK, "{}", month.body);
        assert_eq!(month.body["left_on"], json!(today()));
        assert_eq!(
            month.body["data"]["month"],
            ym(today()),
            "defaults to the month they left"
        );
        assert_eq!(month.body["data"]["attendance"]["staff_id"], json!(s.lan));
        assert_eq!(month.body["data"]["payslip"]["base_salary"], 5_000_000);
        let day = s.get(session, "/me/staff/day").await;
        assert_eq!(day.status, StatusCode::OK, "{}", day.body);
        assert_eq!(
            day.body["data"]["date"],
            json!(today()),
            "defaults to their last day"
        );
        assert_eq!(day.body["data"]["attendance"]["status"], "present");
    }
    // No month after they left.
    let later = next_month_day();
    let after = s
        .get(&s.tech, &format!("/me/staff?month={}", ym(later)))
        .await;
    assert_eq!(after.status, StatusCode::FORBIDDEN, "{}", after.body);
    assert_eq!(after.body["code"], "staff.not_employed_in_month");
    let after_day = s.get(&s.tech, &format!("/me/staff/day?date={later}")).await;
    assert_eq!(
        after_day.status,
        StatusCode::FORBIDDEN,
        "{}",
        after_day.body
    );
    assert_eq!(after_day.body["code"], "staff.not_employed_in_month");
    // Every other screen is refused, whatever the role said.
    for path in [
        "/me/appointments",
        "/customers",
        "/visits",
        "/appointments",
        "/services",
        "/package-plans",
        "/staff",
        "/attendance",
        "/reports/daily",
    ] {
        let r = s.get(&s.tech, path).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path}: {}", r.body);
        assert_eq!(r.body["code"], "auth.forbidden", "{path}");
    }

    // Rehiring gives the role back.
    let back = s.rehire(s.lan, today() + Duration::days(1)).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.body);
    let perms = permissions(&s, &s.tech).await;
    assert!(perms.contains(&"staff.read_own".to_owned()), "{perms:?}");
    assert!(
        !perms.contains(&"staff.read_own_past".to_owned()),
        "{perms:?}"
    );
    assert_eq!(
        s.get(&s.tech, "/me/appointments").await.status,
        StatusCode::OK
    );
    let me = s.get(&s.tech, "/me/staff").await;
    assert_eq!(me.body.get("left_on"), None, "{}", me.body);

    // "Khóa tài khoản": the owner may lock the login outright when ending employment.
    const HUONG_PHONE: &str = "0912100555";
    add_member(
        &s.h.owner,
        "vpyd",
        HUONG_PHONE,
        "KTV Hương",
        SystemRole::Technician,
    )
    .await?;
    let huong = linked_profile(&s, HUONG_PHONE, "Ngô Hương").await?;
    let huong_session = session_for(&s.h, HUONG_PHONE).await;
    let p = s.profile(huong).await;
    let locked = s
        .versioned(
            &s.owner,
            "POST",
            &format!("/staff/{huong}/end"),
            json!({"ended_on": today(), "reason": "Vi phạm nội quy", "lock_login": true}),
            &p["version"],
        )
        .await;
    assert_eq!(locked.status, StatusCode::OK, "{}", locked.body);
    let r = s.get(&huong_session, "/me/staff").await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    assert_eq!(r.body["code"], "auth.not_a_member");
    Ok(())
}

/// The owner's login is never downgraded: ending their own profile is refused, and a profile
/// that has left, linked to the owner's login afterwards, leaves every owner permission intact.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owners_login_is_never_downgraded(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let before = permissions(&s, &s.owner).await;
    assert!(before.contains(&"staff.manage".to_owned()));
    let ended = s.end(s.mai, today(), "Chuyển nơi khác").await;
    assert_eq!(ended.status, StatusCode::OK, "{}", ended.body);
    let owner_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84905123456'")
            .fetch_one(&s.h.owner)
            .await?;
    let p = s.profile(s.mai).await;
    let linked = s
        .versioned(
            &s.owner,
            "PUT",
            &format!("/staff/{}/account", s.mai),
            json!({"user_id": owner_user}),
            &p["version"],
        )
        .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.body);
    assert_eq!(permissions(&s, &s.owner).await, before);
    for path in ["/staff/profiles", "/payroll", "/customers"] {
        assert_eq!(s.get(&s.owner, path).await.status, StatusCode::OK, "{path}");
    }
    Ok(())
}

/// The login link (owner only): the owner never ends their own profile; linking checks
/// membership and uniqueness; unlinking makes the profile count as a technician's for
/// decision C.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_login_link_is_owner_only(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // The owner's own profile: never ended by themselves.
    let own = linked_profile(&s, common::OWNER_PHONE, "Chủ spa").await?;
    let r = s.end(own, today(), "x").await;
    assert_eq!(r.body["code"], "staff.cannot_end_self", "{}", r.body);

    // Link: owner only, an active member, one profile per login.
    let tech_user = s.profile(s.lan).await["details"]["user_id"].clone();
    let mai = s.profile(s.mai).await;
    let taken = s
        .versioned(
            &s.owner,
            "PUT",
            &format!("/staff/{}/account", s.mai),
            json!({"user_id": tech_user}),
            &mai["version"],
        )
        .await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    assert_eq!(taken.body["code"], "staff.account_taken");
    let stranger = s
        .versioned(
            &s.owner,
            "PUT",
            &format!("/staff/{}/account", s.mai),
            json!({"user_id": Uuid::now_v7()}),
            &mai["version"],
        )
        .await;
    assert_eq!(stranger.body["errors"][0]["code"], "not_a_member");
    let by_manager = s
        .versioned(
            &s.manager,
            "PUT",
            &format!("/staff/{}/account", s.lan),
            json!({"user_id": null}),
            &json!(1),
        )
        .await;
    assert_eq!(by_manager.status, StatusCode::FORBIDDEN);

    // Decision C reads the link: Hoa's profile linked to the manager's login is the owner's to
    // record; unlinked, it counts as a technician's.
    let hoa = linked_profile(&s, MANAGER_PHONE, "Quản lý Hoa").await?;
    let blocked = s.attend(&s.manager, hoa, today(), "present").await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    let p = s.profile(hoa).await;
    let unlinked = s
        .versioned(
            &s.owner,
            "PUT",
            &format!("/staff/{hoa}/account"),
            json!({"user_id": null}),
            &p["version"],
        )
        .await;
    assert_eq!(unlinked.status, StatusCode::OK, "{}", unlinked.body);
    assert_eq!(unlinked.body["login_linked"], false);
    assert_eq!(
        s.attend(&s.manager, hoa, today(), "present").await.status,
        StatusCode::OK
    );
    let history = s.get(&s.owner, &format!("/staff/{hoa}/history")).await;
    assert_eq!(history.body[0]["action"], "staff.account_link");
    Ok(())
}
