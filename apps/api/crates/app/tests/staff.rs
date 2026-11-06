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

fn criteria(level: &str) -> Value {
    json!(
        [
            "technique",
            "attitude",
            "customer_feedback",
            "diligence",
            "hygiene",
            "learning"
        ]
        .iter()
        .map(|k| json!({"key": k, "level": level}))
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

/// Spec §10 US9: `GET /me/work` — technician A sees only the customers A served (only A's lines
/// of a shared visit) and the appointments assigned to A; never B's, never prices, phones or
/// notes; unlinked → `linked: false`; another spa sees nothing; managers' routes unchanged.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_sees_only_the_customers_and_bookings_they_serve(pool: PgPool) -> TestResult {
    const TECH_B_PHONE: &str = "0912100444";
    let s = spa(pool).await?;
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
    let unlinked = s.get(&tech_b, "/me/work").await;
    assert_eq!(unlinked.status, StatusCode::OK, "{}", unlinked.body);
    assert_eq!(unlinked.body, json!({"linked": false}));

    let b_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912100444'")
            .fetch_one(&s.h.owner)
            .await?;
    let thu = call(
        &s.h.app,
        "POST",
        "/staff",
        Some(&s.owner),
        Some(json!({"full_name": "Phạm Thu", "position": "Kỹ thuật viên", "user_id": b_user})),
    )
    .await;
    assert_eq!(thu.status, StatusCode::CREATED, "{}", thu.body);
    let thu = id(&thu);

    let hang = s.customer("0905 222 001").await;
    let binh = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.owner),
        Some(
            json!({"full_name": "Đỗ Văn Bình", "phone": "0905 222 002", "gender": "male",
                    "note": "Ghi chú riêng của quầy"}),
        ),
    )
    .await;
    assert_eq!(binh.status, StatusCode::CREATED, "{}", binh.body);
    let binh = id(&binh);
    // Shared visit: one line by Lan (A), one by Thu (B).
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
    let b_only = s.one_off_visit(binh, thu, 1).await;
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

    // A (Lan).
    let a = s.get(&s.tech, "/me/work").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert_eq!(
        a.headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let data = &a.body["data"];
    assert_eq!(a.body["linked"], true);
    let served = data["served"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        served.len(),
        1,
        "voided and B-only visits excluded: {}",
        a.body
    );
    assert_eq!(served[0]["visit_id"], json!(shared));
    assert_eq!(served[0]["customer_name"], "Nguyễn Thị Hằng");
    assert_eq!(
        served[0]["services"],
        json!([{"service_name": "Gội đầu thảo dược", "qty": 1, "from_package": false}]),
        "only A's own line of the shared visit"
    );
    let upcoming = data["upcoming"].as_array().cloned().unwrap_or_default();
    assert_eq!(upcoming.len(), 1, "{}", a.body);
    assert_eq!(upcoming[0]["appointment_id"], book_a.body["id"]);
    assert_eq!(upcoming[0]["customer_name"], "Đỗ Văn Bình");
    assert_eq!(upcoming[0]["status"], "booked");
    assert_eq!(upcoming[0]["services"], json!(["Gội đầu thảo dược"]));
    // Allowlist: exactly these keys, nothing about money, phones, notes or customer ids.
    let keys = |v: &Value| {
        let mut k: Vec<String> = v
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        k.sort();
        k
    };
    assert_eq!(
        keys(&served[0]),
        [
            "business_date",
            "check_in_at",
            "customer_name",
            "services",
            "visit_id"
        ]
    );
    assert_eq!(
        keys(&upcoming[0]),
        [
            "appointment_id",
            "business_date",
            "customer_name",
            "ends_at",
            "services",
            "starts_at",
            "status"
        ]
    );
    let text = a.body.to_string();
    for leak in [
        thu.to_string(),
        b_only.to_string(),
        "Khách mới Yến".to_owned(),
        "Phạm Thu".to_owned(),
        "150000".to_owned(),
        "+84905".to_owned(),
        "đau vai gáy".to_owned(),
        "phòng riêng".to_owned(),
        "Ghi chú riêng".to_owned(),
        hang.to_string(),
        binh.to_string(),
    ] {
        assert!(!text.contains(&leak), "leaked {leak}: {text}");
    }

    // B (Thu): B's two visits (B's 2× line of the shared one), B's guest booking.
    let b = s.get(&tech_b, "/me/work").await;
    assert_eq!(b.status, StatusCode::OK, "{}", b.body);
    let b_served = b.body["data"]["served"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(b_served.len(), 2, "{}", b.body);
    assert_eq!(b_served[0]["visit_id"], json!(b_only), "newest first");
    assert_eq!(b_served[1]["services"][0]["qty"], 2);
    assert_eq!(
        b.body["data"]["upcoming"][0]["customer_name"],
        "Khách mới Yến"
    );
    assert!(!b.body.to_string().contains(&s.lan.to_string()));

    // No way to pick someone else; bad month → 422; another month is empty.
    let pick = s.get(&s.tech, &format!("/me/work?staff_id={thu}")).await;
    assert_eq!(pick.status, StatusCode::BAD_REQUEST, "{}", pick.body);
    assert_eq!(
        s.get(&s.tech, "/me/work?month=2026-13").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let old = s.get(&s.tech, "/me/work?month=2021-01").await;
    assert_eq!(old.body["data"]["served"], json!([]));

    // Another spa's owner (not linked there) sees nothing of ours; anonymous → 401.
    assert_eq!(
        s.get(&s.other, "/me/work").await.body,
        json!({"linked": false})
    );
    let anon = call(&s.h.app, "GET", "/me/work", None, None).await;
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
