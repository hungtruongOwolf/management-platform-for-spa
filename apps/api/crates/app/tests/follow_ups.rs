//! EM round 3 (raw/2026-11-05-em-ui-review-round3.md): re-engagement follow-ups, the widened
//! "lâu không quay lại" list, duplicate-phone check, history / packages search, no-expiry
//! packages, range presets and service categories in display DTOs.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, TimeZone, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::idempotency::hash_request;
use kernel::testing::app_pool;
use kernel::{Actor, TenantId, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";
const PRICE: i64 = 150_000;
const PLAN_PRICE: i64 = 1_200_000;

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    app: PgPool,
    owner: String,
    manager: String,
    tech: String,
    actor: Actor,
    tenant: Uuid,
    staff: Uuid,
    staff_b: Uuid,
    service: Uuid,
    neck: Uuid,
    plan: Uuid,
}

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

fn ict(day: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Ho_Chi_Minh
        .from_local_datetime(&day.and_hms_opt(hour, minute, 0).unwrap_or_default())
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_default()
}

fn id(reply: &Reply) -> Uuid {
    reply.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

fn names(reply: &Reply, list: &str, field: &str) -> BTreeSet<String> {
    reply.body[list]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r[field].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
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
    add_member(&h.owner, "vpyd", TECH_PHONE, "KTV", SystemRole::Technician).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let staff = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let staff_b =
        org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let mut services = Vec::new();
    for (category, service) in [
        ("Gội đầu dưỡng sinh", "Gội đầu thảo dược"),
        ("Chăm sóc cổ vai gáy", "Massage cổ vai gáy"),
    ] {
        let c = call(
            &h.app,
            "POST",
            "/service-categories",
            Some(&owner),
            Some(json!({"name": category})),
        )
        .await;
        let s = call(
            &h.app,
            "POST",
            "/services",
            Some(&owner),
            Some(json!({
                "category_id": id(&c), "name": service, "description": null,
                "duration_min": 60, "recommended": false, "active": true, "prices": {"all": PRICE}
            })),
        )
        .await;
        assert_eq!(s.status, StatusCode::CREATED, "{}", s.body);
        services.push(id(&s));
    }
    let plan = call(&h.app, "POST", "/package-plans", Some(&owner), Some(json!({
        "name": "Gói 10 buổi", "price": PLAN_PRICE, "validity_months": null, "gift_note": null,
        "active": true, "items": [{"service_id": services[0], "sessions": 10, "bonus_sessions": 0}]
    }))).await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let (tenant, user): (Uuid, Uuid) = sqlx::query_as(
        "SELECT m.tenant_id, m.user_id FROM app.membership m JOIN app.user_account u ON u.id = m.user_id
         WHERE u.phone_e164 = '+84905123456'",
    )
    .fetch_one(&h.owner)
    .await?;
    let app = app_pool(&h.owner).await?;
    let (t, u) = (TenantId::from_uuid(tenant), UserId::from_uuid(user));
    let permissions = identity::service::tenant_permissions(&app, u, t)
        .await?
        .ok_or("owner has no membership")?;
    Ok(Spa {
        actor: Actor::new(u, t, permissions, None),
        app,
        owner,
        manager,
        tech,
        tenant,
        staff,
        staff_b,
        service: services[0],
        neck: services[1],
        plan: id(&plan),
        h,
    })
}

impl Spa {
    async fn get(&self, session: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(session), None).await
    }

    async fn post_with_key(&self, session: &str, path: &str, body: Value, key: &str) -> Reply {
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
        self.post_with_key(session, path, body, &Uuid::now_v7().to_string())
            .await
    }

    async fn customer(&self, name: &str, phone: &str) -> Uuid {
        let reply = call(
            &self.h.app,
            "POST",
            "/customers",
            Some(&self.owner),
            Some(json!({"full_name": name, "phone": phone, "gender": "female"})),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        id(&reply)
    }

    /// Sells the plan at `at` (use case with an explicit clock) and returns the sale.
    async fn sell_at(&self, customer: Uuid, at: DateTime<Utc>) -> Setup<Value> {
        let body = json!({
            "customer_id": customer, "closer_staff_id": self.staff,
            "lines": [{"package_plan_id": self.plan}],
            "payments": [{"method": "cash", "amount": PLAN_PRICE}]
        });
        let k = Uuid::now_v7();
        let (_, sale) = sales::service::create_sale_as_of(
            &self.app,
            &self.h.cipher,
            &self.actor,
            k,
            hash_request(k.as_bytes()),
            &serde_json::from_value(body)?,
            at,
        )
        .await
        .map_err(|e| e.code())?;
        Ok(sale)
    }

    /// One visit at `at` (now when `None`), one-off or a package session of `service`.
    async fn visit(
        &self,
        customer: Uuid,
        service: Uuid,
        staff: Uuid,
        one_off: bool,
        at: Option<DateTime<Utc>>,
    ) -> Setup<Value> {
        let (source, payments) = if one_off {
            ("one_off", json!([{"method": "cash", "amount": PRICE}]))
        } else {
            ("package", json!([]))
        };
        let body = json!({"customer_id": customer, "payments": payments,
            "lines": [{"service_id": service, "source": source, "staff_id": staff}]});
        match at {
            None => {
                let reply = self.post(&self.owner, "/visits", body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                Ok(reply.body)
            }
            Some(at) => {
                let k = Uuid::now_v7();
                let (_, visit) = operations::service::record_as_of(
                    &self.app,
                    &self.h.cipher,
                    &self.actor,
                    k,
                    hash_request(k.as_bytes()),
                    &serde_json::from_value(body)?,
                    at,
                )
                .await
                .map_err(|e| e.code())?;
                Ok(visit)
            }
        }
    }

    /// A paper-book package (owner, transitional route); returns its id.
    async fn import(
        &self,
        customer: Uuid,
        starts_on: NaiveDate,
        expires_on: Option<NaiveDate>,
        remaining: u32,
    ) -> String {
        let reply = self
            .post(
                &self.owner,
                "/opening-balances",
                json!({"customer_id": customer, "packages": [{
                    "name": "Gói sổ giấy", "sold_price": 1_000_000, "starts_on": starts_on,
                    "expires_on": expires_on,
                    "items": [{"service_id": self.service, "sessions_purchased": 10, "sessions_remaining": remaining}]
                }]}),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        reply.body["customer_packages"][0]["id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    async fn follow_up(&self, customer: Uuid, body: Value) -> Reply {
        self.post(
            &self.manager,
            &format!("/customers/{customer}/follow-ups"),
            body,
        )
        .await
    }

    async fn other_spa_owner(&self) -> Setup<String> {
        create_tenant(&self.h.owner, "other", "Spa khác", "Huế").await?;
        add_member(
            &self.h.owner,
            "other",
            OTHER_OWNER_PHONE,
            "Chủ khác",
            SystemRole::Owner,
        )
        .await?;
        Ok(session_for(&self.h, OTHER_OWNER_PHONE).await)
    }
}

// ---------------------------------------------------------------------------- follow-ups

/// Recording a follow-up: who may, who it is attributed to, idempotency, the default rule,
/// validation, audit, append-only and tenant isolation.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_follow_up_is_recorded_once_by_the_session_user(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let path = format!("/customers/{ha}/follow-ups");
    let key = Uuid::now_v7().to_string();
    let body = json!({"channel": "zalo", "outcome": "no_answer", "note": "Gọi 2 lần"});

    let first = s.post_with_key(&s.manager, &path, body.clone(), &key).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.body["contacted_on"], json!(day));
    assert_eq!(
        first.body["next_follow_up_on"],
        json!(day + Duration::days(3)),
        "no answer → retry in 3 days"
    );
    assert_eq!(first.body["by_name"], "Quản lý Hoa");
    assert_eq!(first.body["channel"], "zalo");
    // Retry with the same key and body: replayed, not recorded twice.
    let again = s.post_with_key(&s.manager, &path, body.clone(), &key).await;
    assert_eq!(
        (again.status, again.body["id"].clone()),
        (StatusCode::CREATED, first.body["id"].clone())
    );
    let reuse = s
        .post_with_key(
            &s.manager,
            &path,
            json!({"channel": "phone", "outcome": "other"}),
            &key,
        )
        .await;
    assert_eq!(reuse.body["code"], "idempotency.key_reuse_mismatch");
    let no_key = call(
        &s.h.app,
        "POST",
        &path,
        Some(&s.manager),
        Some(body.clone()),
    )
    .await;
    assert_eq!(no_key.status, StatusCode::BAD_REQUEST);

    // The recorder comes from the session; a body cannot name one.
    let forged = s
        .follow_up(
            ha,
            json!({"channel": "phone", "outcome": "other", "created_by": Uuid::now_v7()}),
        )
        .await;
    assert_eq!(forged.status, StatusCode::UNPROCESSABLE_ENTITY);

    // The next date given overrides the default; owner may record too.
    let planned = s
        .post(
            &s.owner,
            &path,
            json!({"channel": "phone", "outcome": "no_answer",
                   "next_follow_up_on": day + Duration::days(10)}),
        )
        .await;
    assert_eq!(planned.status, StatusCode::CREATED, "{}", planned.body);
    assert_eq!(
        planned.body["next_follow_up_on"],
        json!(day + Duration::days(10))
    );
    let will_come = s
        .follow_up(ha, json!({"channel": "in_person", "outcome": "will_come"}))
        .await;
    assert_eq!(
        will_come.body["next_follow_up_on"],
        json!(day + Duration::days(7)),
        "will come → check in a week"
    );
    let list = s.get(&s.manager, &path).await;
    assert_eq!(
        list.body["suggested_next_on_by_outcome"],
        json!({"will_come": day + Duration::days(7), "not_interested": null,
               "no_answer": day + Duration::days(3), "other": null})
    );
    assert_eq!(list.body["today"], json!(day));
    assert_eq!(list.body["not_interested_pause_days"], 90);

    for (bad, field, code) in [
        (
            json!({"channel": "phone", "outcome": "other", "contacted_on": day + Duration::days(1)}),
            "contacted_on",
            "in_future",
        ),
        (
            json!({"channel": "phone", "outcome": "other", "contacted_on": day - Duration::days(31)}),
            "contacted_on",
            "too_old",
        ),
        (
            json!({"channel": "phone", "outcome": "other", "next_follow_up_on": day - Duration::days(1)}),
            "next_follow_up_on",
            "before_contact",
        ),
        (
            json!({"channel": "phone", "outcome": "other", "note": "x".repeat(501)}),
            "note",
            "too_long",
        ),
    ] {
        let r = s.follow_up(ha, bad).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.body);
        assert_eq!(r.body["errors"][0]["field"], field, "{}", r.body);
        assert_eq!(r.body["errors"][0]["code"], code, "{}", r.body);
    }
    let sms = s
        .follow_up(ha, json!({"channel": "sms", "outcome": "other"}))
        .await;
    assert_eq!(sms.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Newest first; three recorded (the replay and the rejects added nothing).
    let list = s.get(&s.manager, &path).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert_eq!(list.body["items"].as_array().map(Vec::len), Some(3));
    assert_eq!(list.body["items"][0]["outcome"], "will_come");
    assert_eq!(list.body["has_more"], false);

    // Audited without the note text.
    let audits: Vec<Value> = sqlx::query_scalar(
        "SELECT after FROM app.audit_log WHERE action = 'customer.follow_up' AND entity_id = $1",
    )
    .bind(ha)
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(audits.len(), 3);
    assert!(audits.iter().any(|a| a["has_note"] == true));
    assert!(audits.iter().all(|a| !a.to_string().contains("Gọi 2 lần")));

    // Who may: technician 403, anonymous 401, another spa 404 (and sees none of it).
    let body = json!({"channel": "phone", "outcome": "other"});
    assert_eq!(
        s.post(&s.tech, &path, body.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(s.get(&s.tech, &path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        call(&s.h.app, "GET", &path, None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let intruder = s.other_spa_owner().await?;
    assert_eq!(
        s.post(&intruder, &path, body).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(s.get(&intruder, &path).await.status, StatusCode::NOT_FOUND);
    Ok(())
}

/// The table is append-only and tenant-scoped for the runtime role (RLS), like the books.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn follow_ups_are_append_only_and_isolated_by_rls(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let r = s
        .follow_up(ha, json!({"channel": "phone", "outcome": "no_answer"}))
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);

    let mut tx = s.app.begin().await?;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(s.tenant.to_string())
        .execute(&mut *tx)
        .await?;
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM app.customer_follow_up")
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(visible, 1);
    let update = sqlx::query("UPDATE app.customer_follow_up SET outcome = 'will_come'")
        .execute(&mut *tx)
        .await;
    assert!(update.is_err(), "append-only");
    tx.rollback().await?;

    let mut tx = s.app.begin().await?;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(Uuid::now_v7().to_string())
        .execute(&mut *tx)
        .await?;
    let foreign: i64 = sqlx::query_scalar("SELECT count(*) FROM app.customer_follow_up")
        .fetch_one(&mut *tx)
        .await?;
    assert_eq!(foreign, 0, "another tenant sees nothing");
    tx.rollback().await?;
    // No tenant set: deny by default.
    let none: i64 = sqlx::query_scalar("SELECT count(*) FROM app.customer_follow_up")
        .fetch_one(&s.app)
        .await?;
    assert_eq!(none, 0);
    Ok(())
}

// ------------------------------------------------------------------ "lâu không quay lại"

/// One-off customers and package customers (sessions left or used up) are both listed, from
/// the latest of last visit / last sale / package start; the N-day boundary is inclusive.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn inactive_covers_one_off_and_package_customers(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let back = |n: i64| day - Duration::days(n);

    let one_off = s.customer("Phạm Thị Lẻ", "0905111001").await;
    s.visit(
        one_off,
        s.service,
        s.staff,
        true,
        Some(ict(back(10), 10, 0)),
    )
    .await?;
    let with_sessions = s.customer("Trần Gói Còn", "0905111002").await;
    s.sell_at(with_sessions, ict(back(20), 9, 0)).await?;
    s.visit(
        with_sessions,
        s.service,
        s.staff,
        false,
        Some(ict(back(9), 9, 0)),
    )
    .await?;
    let used_up = s.customer("Lê Gói Hết", "0905111003").await;
    s.import(used_up, back(30), None, 0).await;
    let boundary = s.customer("Khách đúng 7 ngày", "0905111004").await;
    s.visit(
        boundary,
        s.service,
        s.staff,
        true,
        Some(ict(back(7), 23, 59)),
    )
    .await?;
    let recent = s.customer("Khách 6 ngày", "0905111005").await;
    s.visit(recent, s.service, s.staff, true, Some(ict(back(6), 0, 1)))
        .await?;
    let bought_recently = s.customer("Khách mới mua", "0905111006").await;
    s.visit(
        bought_recently,
        s.service,
        s.staff,
        true,
        Some(ict(back(40), 9, 0)),
    )
    .await?;
    s.sell_at(bought_recently, ict(back(2), 9, 0)).await?;
    s.customer("Khách chưa đến", "0905111007").await;

    let all = s.get(&s.manager, "/customers/inactive").await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.body);
    assert_eq!(
        names(&all, "items", "full_name"),
        set(&[
            "Phạm Thị Lẻ",
            "Trần Gói Còn",
            "Lê Gói Hết",
            "Khách đúng 7 ngày"
        ]),
        "default days=7, scope=all"
    );
    let row = |name: &str| -> Value {
        all.body["items"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["full_name"] == name).cloned())
            .unwrap_or(Value::Null)
    };
    assert_eq!(row("Phạm Thị Lẻ")["kind"], "one_off");
    assert_eq!(row("Phạm Thị Lẻ")["gender"], "female");
    assert_eq!(row("Trần Gói Còn")["kind"], "package");
    assert_eq!(row("Trần Gói Còn")["last_seen_on"], json!(back(9)));
    assert_eq!(row("Trần Gói Còn")["sessions_remaining"], 9);
    assert_eq!(row("Lê Gói Hết")["kind"], "package");
    assert_eq!(row("Lê Gói Hết")["last_seen_on"], json!(back(30)));
    assert_eq!(row("Lê Gói Hết")["active_packages"], 0);
    assert_eq!(row("Khách đúng 7 ngày")["days_absent"], 7);
    assert_eq!(
        all.body["items"][0]["full_name"], "Lê Gói Hết",
        "longest first"
    );

    let packages = s.get(&s.manager, "/customers/inactive?kind=package").await;
    assert_eq!(
        names(&packages, "items", "full_name"),
        set(&["Trần Gói Còn", "Lê Gói Hết"])
    );
    let one_offs = s.get(&s.manager, "/customers/inactive?kind=one_off").await;
    assert_eq!(
        names(&one_offs, "items", "full_name"),
        set(&["Phạm Thị Lẻ", "Khách đúng 7 ngày"])
    );
    let sessions = s
        .get(&s.manager, "/customers/inactive?scope=with_sessions")
        .await;
    assert_eq!(
        names(&sessions, "items", "full_name"),
        set(&["Trần Gói Còn"])
    );
    let by_name = s.get(&s.manager, "/customers/inactive?q=pham%20thi").await;
    assert_eq!(names(&by_name, "items", "full_name"), set(&["Phạm Thị Lẻ"]));
    let by_phone = s.get(&s.manager, "/customers/inactive?q=111003").await;
    assert_eq!(names(&by_phone, "items", "full_name"), set(&["Lê Gói Hết"]));
    assert_eq!(
        s.get(&s.manager, "/customers/inactive?q=a").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.get(&s.tech, "/customers/inactive").await.status,
        StatusCode::FORBIDDEN
    );
    let intruder = s.other_spa_owner().await?;
    let theirs = s.get(&intruder, "/customers/inactive?days=1").await;
    assert_eq!(theirs.body["items"], json!([]));
    Ok(())
}

/// `follow_up=due|not_contacted|scheduled|all`, judged on the current absence only.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_follow_up_filter_tells_who_to_call_today(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let mut ids = Vec::new();
    for (name, phone) in [
        ("A hẹn gọi lại", "0905111001"),
        ("B đến hạn", "0905111002"),
        ("C chưa gọi", "0905111003"),
        ("D gọi trước lần đến", "0905111004"),
        ("E không muốn", "0905111005"),
    ] {
        let c = s.customer(name, phone).await;
        let at = if name.starts_with('D') {
            ict(day - Duration::days(10), 10, 0)
        } else {
            ict(day - Duration::days(12), 10, 0)
        };
        s.visit(c, s.service, s.staff, true, Some(at)).await?;
        ids.push(c);
    }
    let ok = |r: Reply| assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    ok(
        s.follow_up(ids[0], json!({"channel": "phone", "outcome": "no_answer"}))
            .await,
    );
    ok(s.follow_up(
        ids[1],
        json!({"channel": "phone", "outcome": "will_come", "contacted_on": day - Duration::days(2),
               "next_follow_up_on": day}),
    )
    .await);
    // D was called on day −12, then came on day −10: that call belongs to the last absence.
    ok(s.follow_up(
        ids[3],
        json!({"channel": "phone", "outcome": "not_interested", "contacted_on": day - Duration::days(12)}),
    )
    .await);
    ok(s.follow_up(
        ids[4],
        json!({"channel": "zalo", "outcome": "not_interested"}),
    )
    .await);
    // F said "not interested" 95 days ago (older than the API accepts, so written directly):
    // past the 90-day pause, F is due once more.
    let f = s.customer("F hết thời gian nghỉ", "0905111006").await;
    s.visit(
        f,
        s.service,
        s.staff,
        true,
        Some(ict(day - Duration::days(100), 10, 0)),
    )
    .await?;
    sqlx::query(
        "INSERT INTO app.customer_follow_up (id, tenant_id, customer_id, contacted_on, channel, outcome, created_by)
         SELECT $1, $2, $3, $4, 'phone', 'not_interested', u.id FROM app.user_account u
         WHERE u.phone_e164 = '+84912000111'",
    )
    .bind(Uuid::now_v7())
    .bind(s.tenant)
    .bind(f)
    .bind(day - Duration::days(95))
    .execute(&s.h.owner)
    .await?;

    let all = inactive(&s, "follow_up=all").await;
    assert_eq!(names(&all, "items", "full_name").len(), 6);
    assert_eq!(
        names(&inactive(&s, "follow_up=due").await, "items", "full_name"),
        set(&[
            "B đến hạn",
            "C chưa gọi",
            "D gọi trước lần đến",
            "F hết thời gian nghỉ"
        ]),
        "E said not interested today: out of due for 90 days"
    );
    assert_eq!(
        names(
            &inactive(&s, "follow_up=not_contacted").await,
            "items",
            "full_name"
        ),
        set(&["C chưa gọi", "D gọi trước lần đến"])
    );
    assert_eq!(
        names(
            &inactive(&s, "follow_up=scheduled").await,
            "items",
            "full_name"
        ),
        set(&["A hẹn gọi lại"])
    );
    let a = all.body["items"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|r| r["full_name"] == "A hẹn gọi lại")
                .cloned()
        })
        .unwrap_or(Value::Null);
    assert_eq!(
        a["last_follow_up"],
        json!({"on": day, "channel": "phone", "outcome": "no_answer", "by": "Quản lý Hoa"})
    );
    assert_eq!(a["next_follow_up_on"], json!(day + Duration::days(3)));
    let d = all.body["items"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|r| r["full_name"] == "D gọi trước lần đến")
                .cloned()
        })
        .unwrap_or(Value::Null);
    assert_eq!(d["last_follow_up"], Value::Null);
    assert_eq!(d["next_follow_up_on"], Value::Null);
    let due_on = |name: &str| -> Value {
        all.body["items"]
            .as_array()
            .and_then(|a| a.iter().find(|r| r["full_name"] == name).cloned())
            .map(|r| r["due_on"].clone())
            .unwrap_or(Value::Null)
    };
    assert_eq!(due_on("A hẹn gọi lại"), json!(day + Duration::days(3)));
    assert_eq!(due_on("E không muốn"), json!(day + Duration::days(90)));
    assert_eq!(
        due_on("F hết thời gian nghỉ"),
        json!(day - Duration::days(5))
    );
    assert_eq!(due_on("C chưa gọi"), Value::Null);

    // Sort: who to call first. Never contacted (longest absence first), then the planned date,
    // then contacted with nothing planned. `follow_up=due|scheduled` sorts by due by default.
    let order = |r: &Reply| -> Vec<String> {
        r.body["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| x["full_name"].as_str().map(|n| n[..1].to_owned()))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        order(&inactive(&s, "sort=due").await),
        ["C", "D", "F", "B", "A", "E"]
    );
    assert_eq!(
        order(&inactive(&s, "follow_up=due").await),
        ["C", "D", "F", "B"]
    );
    assert_eq!(order(&inactive(&s, "follow_up=scheduled").await), ["A"]);
    let absent = order(&inactive(&s, "").await);
    assert_eq!(
        absent.last().map(String::as_str),
        Some("D"),
        "absence order by default"
    );
    let due_by_absence: Vec<String> = absent
        .iter()
        .filter(|n| ["B", "C", "D", "F"].contains(&n.as_str()))
        .cloned()
        .collect();
    assert_eq!(
        order(&inactive(&s, "follow_up=due&sort=absent").await),
        due_by_absence
    );
    // Paging in due order neither skips nor repeats.
    let mut seen = Vec::new();
    let mut cursor = String::new();
    loop {
        let q = if cursor.is_empty() {
            "sort=due&limit=2".to_owned()
        } else {
            format!("sort=due&limit=2&cursor={cursor}")
        };
        let page = inactive(&s, &q).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        seen.extend(order(&page));
        match page.body["next_cursor"].as_str() {
            Some(next) => cursor = next.to_owned(),
            None => break,
        }
    }
    assert_eq!(seen, ["C", "D", "F", "B", "A", "E"]);
    Ok(())
}

async fn inactive(s: &Spa, query: &str) -> Reply {
    s.get(&s.manager, &format!("/customers/inactive?{query}"))
        .await
}

// ------------------------------------------------------------------------------ phone check

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn phone_check_finds_duplicates_like_create_does(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905 111 222").await;
    for typed in ["0905111222", "+84 905 111 222", "0905 111 222"] {
        let r = s
            .get(
                &s.manager,
                &format!(
                    "/customers/phone-check?phone={}",
                    typed.replace('+', "%2B").replace(' ', "%20")
                ),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(
            r.body,
            json!({"valid": true, "taken": true, "existing_id": ha, "full_name": "Nguyễn Thị Hà"}),
            "{typed}"
        );
    }
    let free = s
        .get(&s.manager, "/customers/phone-check?phone=0905999888")
        .await;
    assert_eq!(free.body, json!({"valid": true, "taken": false}));
    let partial = s.get(&s.manager, "/customers/phone-check?phone=0905").await;
    assert_eq!(partial.body, json!({"valid": false, "taken": false}));
    let empty = s.get(&s.manager, "/customers/phone-check").await;
    assert_eq!(empty.body, json!({"valid": false, "taken": false}));
    let long = format!("/customers/phone-check?phone={}", "9".repeat(101));
    assert_eq!(
        s.get(&s.manager, &long).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.get(&s.tech, "/customers/phone-check?phone=0905111222")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    // Another spa's customers are not visible: the same number is free there.
    let intruder = s.other_spa_owner().await?;
    let theirs = s
        .get(&intruder, "/customers/phone-check?phone=0905111222")
        .await;
    assert_eq!(theirs.body, json!({"valid": true, "taken": false}));
    // Create still refuses the duplicate.
    let dup = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.manager),
        Some(json!({"full_name": "Hà khác", "phone": "+84905111222"})),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    assert_eq!(dup.body["code"], "customer.phone_taken");
    Ok(())
}

// ------------------------------------------------------------------------- history search

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn history_filters_by_text_kind_and_day(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    s.visit(
        ha,
        s.service,
        s.staff,
        true,
        Some(ict(day - Duration::days(2), 10, 0)),
    )
    .await?;
    s.visit(ha, s.neck, s.staff_b, true, None).await?;
    let sale = s
        .post(
            &s.owner,
            "/sales",
            json!({
                "customer_id": ha, "closer_staff_id": s.staff,
                "lines": [{"package_plan_id": s.plan}],
                "payments": [{"method": "cash", "amount": PLAN_PRICE}]
            }),
        )
        .await;
    assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);

    let kinds = |r: &Reply| -> Vec<String> {
        r.body["events"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| e["kind"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let base = format!("/customers/{ha}/history");
    let all = s.get(&s.manager, &base).await;
    // Same-day order mixes the API and database clocks (backend §3b), so compare as a set here.
    let mut every = kinds(&all);
    every.sort();
    assert_eq!(every, ["purchase", "visit", "visit"]);
    let first_line = |r: &Reply| r.body["events"][0]["visit"]["lines"][0].clone();

    let herbal = s.get(&s.manager, &format!("{base}?q=thao%20duoc")).await;
    assert_eq!(kinds(&herbal), ["visit"], "service name, accents optional");
    assert_eq!(first_line(&herbal)["service_name"], "Gội đầu thảo dược");
    assert_eq!(first_line(&herbal)["category_name"], "Gội đầu dưỡng sinh");
    let neck = s.get(&s.manager, &format!("{base}?q=CO%20VAI")).await;
    assert_eq!(kinds(&neck), ["visit"]);
    assert_eq!(first_line(&neck)["staff_name"], "KTV Mai");
    let by_staff = s.get(&s.manager, &format!("{base}?q=ktv%20mai")).await;
    assert_eq!(kinds(&by_staff), ["visit"]);
    let by_category = s.get(&s.manager, &format!("{base}?q=duong%20sinh")).await;
    assert_eq!(kinds(&by_category), ["visit"], "category of the service");
    let package = s.get(&s.manager, &format!("{base}?q=goi%2010")).await;
    assert_eq!(kinds(&package), ["purchase"]);
    let closer = s.get(&s.manager, &format!("{base}?q=ktv%20lan")).await;
    assert_eq!(
        kinds(&closer),
        ["purchase", "visit"],
        "closer and technician"
    );

    let purchases = s.get(&s.manager, &format!("{base}?kind=purchase")).await;
    assert_eq!(kinds(&purchases), ["purchase"]);
    let d2 = day - Duration::days(2);
    let that_day = s
        .get(&s.manager, &format!("{base}?from={d2}&to={d2}"))
        .await;
    assert_eq!(kinds(&that_day), ["visit"]);
    let since = s
        .get(&s.manager, &format!("{base}?from={day}&kind=visit"))
        .await;
    assert_eq!(kinds(&since), ["visit"]);
    for bad in [
        format!("{base}?from={day}&to={d2}"),
        format!("{base}?q=a"),
        format!("{base}?q={}", "x".repeat(101)),
        format!("{base}?kind=refund"),
    ] {
        assert!(
            s.get(&s.manager, &bad).await.status.is_client_error(),
            "{bad}"
        );
    }
    Ok(())
}

// ---------------------------------------------------- packages: no expiry and name search

/// A package without an expiry date never expires: it is never "expiring soon" or "expired",
/// however old it is, in the counts, the filters or the package itself.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn packages_without_expiry_never_expire(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let old = s.customer("Khách gói cũ", "0905111001").await;
    let unlimited = s.import(old, day - Duration::days(3 * 365), None, 5).await;
    let soon = s.customer("Khách sắp hết hạn", "0905111002").await;
    s.import(
        soon,
        day - Duration::days(30),
        Some(day + Duration::days(5)),
        5,
    )
    .await;
    let gone = s.customer("Khách hết hạn", "0905111003").await;
    s.import(
        gone,
        day - Duration::days(400),
        Some(day - Duration::days(1)),
        5,
    )
    .await;
    let sold = s.customer("Khách mua gói", "0905111004").await;
    s.sell_at(sold, ict(day, 9, 0)).await?;

    let overview = s.get(&s.manager, "/packages").await;
    assert_eq!(overview.status, StatusCode::OK, "{}", overview.body);
    assert_eq!(
        overview.body["counts"],
        json!({"active": 3, "expiring_soon": 1, "expired": 1, "exhausted": 0, "cancelled": 0})
    );
    let widest = s
        .get(&s.manager, "/packages?expiring_within_days=365")
        .await;
    assert_eq!(
        names(&widest, "items", "customer_name"),
        set(&["Khách sắp hết hạn"])
    );
    assert_eq!(widest.body["counts"]["expiring_soon"], 1);
    let expired = s.get(&s.manager, "/packages?status=expired").await;
    assert_eq!(
        names(&expired, "items", "customer_name"),
        set(&["Khách hết hạn"])
    );
    let active = s.get(&s.manager, "/packages?status=active").await;
    assert_eq!(
        names(&active, "items", "customer_name"),
        set(&["Khách gói cũ", "Khách sắp hết hạn", "Khách mua gói"])
    );
    let row = active.body["items"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|r| r["customer_name"] == "Khách gói cũ")
                .cloned()
        })
        .unwrap_or(Value::Null);
    assert_eq!(
        (row["expires_on"].clone(), row["days_to_expiry"].clone()),
        (Value::Null, Value::Null)
    );
    assert_eq!(row["customer_gender"], "female");
    let package = s
        .get(&s.manager, &format!("/customer-packages/{unlimited}"))
        .await;
    assert_eq!(package.body["status"], "active");
    assert_eq!(
        package.body["items"][0]["category_name"],
        "Gội đầu dưỡng sinh"
    );

    // q also searches the package name (accent-insensitive), besides customer name and phone.
    let paper = s.get(&s.manager, "/packages?q=so%20giay").await;
    assert_eq!(names(&paper, "items", "customer_name").len(), 3);
    assert_eq!(paper.body["counts"]["active"], 2, "counts follow q");
    let plan = s.get(&s.manager, "/packages?q=GOI%2010").await;
    assert_eq!(
        names(&plan, "items", "customer_name"),
        set(&["Khách mua gói"])
    );
    let by_customer = s.get(&s.manager, "/packages?q=het%20han").await;
    assert_eq!(
        names(&by_customer, "items", "customer_name"),
        set(&["Khách sắp hết hạn", "Khách hết hạn"])
    );
    Ok(())
}

// ------------------------------------------------------------------------- range presets

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn range_presets_cover_week_month_year_and_years(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = today();
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    s.visit(ha, s.service, s.staff, true, None).await?;
    let long_ago = day - Duration::days(300);
    s.visit(ha, s.neck, s.staff, true, Some(ict(long_ago, 10, 0)))
        .await?;

    let jan = |y: i32| NaiveDate::from_ymd_opt(y, 1, 1).unwrap_or_default();
    let last_month_start = first_of_month(day)
        .checked_sub_months(Months::new(1))
        .unwrap_or_default();
    let twelve = first_of_month(day)
        .checked_sub_months(Months::new(11))
        .unwrap_or_default();
    for (preset, from, to, granularity, buckets) in [
        ("last_7_days", day - Duration::days(6), day, "day", Some(7)),
        (
            "last_30_days",
            day - Duration::days(29),
            day,
            "day",
            Some(30),
        ),
        ("this_month", first_of_month(day), day, "day", None),
        (
            "last_month",
            last_month_start,
            first_of_month(day).pred_opt().unwrap_or_default(),
            "day",
            None,
        ),
        (
            "this_year",
            jan(day.year()),
            day,
            "month",
            Some(day.month() as usize),
        ),
        ("last_12_months", twelve, day, "month", Some(12)),
        ("last_2_years", jan(day.year() - 1), day, "month", None),
    ] {
        let r = s
            .get(&s.owner, &format!("/reports/range?preset={preset}"))
            .await;
        assert_eq!(r.status, StatusCode::OK, "{preset}: {}", r.body);
        assert_eq!(
            (
                r.body["from"].clone(),
                r.body["to"].clone(),
                r.body["granularity"].clone(),
                r.body["preset"].clone()
            ),
            (json!(from), json!(to), json!(granularity), json!(preset)),
            "{preset}"
        );
        if let Some(n) = buckets {
            assert_eq!(
                r.body["series"].as_array().map(Vec::len),
                Some(n),
                "{preset}"
            );
        }
    }

    // Two years by year: both visits; every point has what a line chart needs (customers,
    // visits, net revenue).
    let years = s
        .get(
            &s.owner,
            "/reports/range?preset=last_2_years&granularity=year",
        )
        .await;
    assert_eq!(years.body["series"].as_array().map(Vec::len), Some(2));
    let series = years.body["series"].as_array().cloned().unwrap_or_default();
    let visits: i64 = series
        .iter()
        .filter_map(|b| b["counts"]["visits"].as_i64())
        .sum();
    assert_eq!(visits, 2);
    assert_eq!(years.body["totals"]["visits"], 2);
    for b in &series {
        assert_eq!(b["net_revenue"], b["revenue"]["net"]["total"]);
        assert!(b["counts"]["customers"].is_i64());
    }
    let this_year = series.last().cloned().unwrap_or(Value::Null);
    assert_eq!(this_year["start"], json!(jan(day.year())));
    assert!(this_year["counts"]["visits"].as_i64() >= Some(1));
    let top: BTreeSet<String> = years.body["top_services"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|t| t["category_name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(top, set(&["Gội đầu dưỡng sinh", "Chăm sóc cổ vai gáy"]));
    assert_eq!(
        years.body["technicians"],
        json!([{"staff_id": s.staff, "name": "KTV Lan", "visits": 2, "sessions_used": 0,
                "one_off_services": 2}]),
        "per technician over the whole range"
    );
    let week = s.get(&s.owner, "/reports/range?preset=last_7_days").await;
    assert_eq!(
        week.body["technicians"][0]["visits"], 1,
        "only inside the range"
    );

    // Granularity overrides the preset's default; spans are bounded per granularity.
    let by_week = s
        .get(
            &s.owner,
            "/reports/range?preset=last_30_days&granularity=week",
        )
        .await;
    assert_eq!(by_week.body["granularity"], "week");
    let from = day - Duration::days(700);
    let months = s
        .get(
            &s.owner,
            &format!("/reports/range?from={from}&to={day}&granularity=month"),
        )
        .await;
    assert_eq!(months.status, StatusCode::OK, "{}", months.body);
    for bad in [
        format!("/reports/range?from={from}&to={day}&granularity=day"),
        format!(
            "/reports/range?from={}&to={day}&granularity=year",
            day - Duration::days(1830)
        ),
        format!("/reports/range?preset=this_month&from={day}"),
        "/reports/range?preset=forever".to_owned(),
    ] {
        let r = s.get(&s.owner, &bad).await;
        assert!(r.status.is_client_error(), "{bad}: {}", r.body);
    }

    // 3–5 years: the owner only (report.long_range, EM Q5); the manager keeps the 2-year cap.
    let five = s.get(&s.owner, "/reports/range?preset=last_5_years").await;
    assert_eq!(five.status, StatusCode::OK, "{}", five.body);
    assert_eq!(
        (five.body["from"].clone(), five.body["granularity"].clone()),
        (json!(jan(day.year() - 4)), json!("year"))
    );
    assert_eq!(five.body["series"].as_array().map(Vec::len), Some(5));
    let three = s.get(&s.owner, "/reports/range?preset=last_3_years").await;
    assert_eq!(
        (
            three.body["from"].clone(),
            three.body["granularity"].clone()
        ),
        (json!(jan(day.year() - 2)), json!("month"))
    );
    let four_years = format!(
        "/reports/range?from={}&to={day}&granularity=month",
        day - Duration::days(4 * 365)
    );
    assert_eq!(s.get(&s.owner, &four_years).await.status, StatusCode::OK);
    for path in [
        "/reports/range?preset=last_5_years".to_owned(),
        "/reports/range?preset=last_3_years".to_owned(),
        four_years,
        format!(
            "/reports/range?from={}&to={day}&granularity=year",
            day - Duration::days(732)
        ),
    ] {
        let r = s.get(&s.manager, &path).await;
        assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(r.body["errors"][0]["code"], "range_too_long", "{path}");
    }
    let two = s
        .get(&s.manager, "/reports/range?preset=last_2_years")
        .await;
    assert_eq!(two.status, StatusCode::OK, "manager keeps 2 years");
    let conflict = s
        .get(
            &s.owner,
            &format!("/reports/range?preset=this_month&to={day}"),
        )
        .await;
    assert_eq!(conflict.body["errors"][0]["code"], "conflicts_with_range");
    Ok(())
}

// -------------------------------------------------------------- categories and folding

/// Every display DTO carrying a service name also carries its category.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn service_names_come_with_their_category(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ha = s.customer("Nguyễn Thị Hà", "0905111001").await;
    let sale = s
        .post(
            &s.owner,
            "/sales",
            json!({
                "customer_id": ha, "closer_staff_id": s.staff,
                "lines": [{"package_plan_id": s.plan}],
                "payments": [{"method": "cash", "amount": PLAN_PRICE}]
            }),
        )
        .await;
    let package = sale.body["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(
        sale.body["customer_packages"][0]["items"][0]["category_name"],
        "Gội đầu dưỡng sinh"
    );
    let visit = s.visit(ha, s.service, s.staff, false, None).await?;
    assert_eq!(visit["lines"][0]["category_name"], "Gội đầu dưỡng sinh");
    let today_list = s.get(&s.manager, "/visits").await;
    assert_eq!(
        today_list.body[0]["lines"][0]["category_name"], "Gội đầu dưỡng sinh",
        "{}",
        today_list.body
    );
    assert_eq!(today_list.body[0]["customer_gender"], "female");
    let ledger = s.get(&s.owner, "/reports/ledger").await;
    assert_eq!(
        ledger.body["rows"][0]["customer_gender"], "female",
        "{}",
        ledger.body
    );
    let usage = s
        .get(&s.manager, &format!("/customer-packages/{package}/usage"))
        .await;
    assert_eq!(usage.body["uses"][0]["category_name"], "Gội đầu dưỡng sinh");
    let packages = s
        .get(&s.manager, &format!("/customers/{ha}/packages"))
        .await;
    assert_eq!(
        packages.body[0]["items"][0]["category_name"], "Gội đầu dưỡng sinh",
        "{}",
        packages.body
    );
    Ok(())
}

/// `app.search_fold` (SQL, for snapshots) agrees with `crm::text::search_key` (Rust, for
/// `customer.search_text`) on every Vietnamese letter, both cases, NFC and NFD.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn sql_folding_matches_the_rust_search_key(pool: PgPool) -> TestResult {
    let letters = "àáảãạăằắẳẵặâầấẩẫậèéẻẽẹêềếểễệìíỉĩịòóỏõọôồốổỗộơờớởỡợùúủũụưừứửữựỳýỷỹỵđ";
    let samples = [
        letters.to_owned(),
        letters.to_uppercase(),
        "  Gội   ĐẦU  dưỡng sinh ".to_owned(),
        "Ha\u{0300} No\u{0323}i".to_owned(),
        "Chăm sóc cổ vai gáy".to_owned(),
    ];
    for sample in samples {
        let folded: String = sqlx::query_scalar("SELECT app.search_fold($1)")
            .bind(&sample)
            .fetch_one(&pool)
            .await?;
        assert_eq!(folded, crm::text::search_key(&sample), "{sample}");
    }
    Ok(())
}
