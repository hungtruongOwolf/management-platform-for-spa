//! "Sửa lượt" (spec R1-1 US6b, AC6b.1–6b.13): direct edits through `PATCH /visits/{id}` and
//! corrections of money or sessions through `POST /visits/{id}/correct` (void + record, one
//! transaction).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECHNICIAN_PHONE: &str = "0912000222";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    technician: String,
    /// Female customer with the 12-session shampoo package.
    ha: String,
    /// Another customer (the visit was recorded for the wrong person).
    binh: String,
    lan: String,
    mai: String,
    /// Shampoo, flat 49.000đ (also the package's service).
    shampoo: String,
    /// "Cơ Bản – Dầu cặp", flat 39.000đ.
    cheaper: String,
    /// "Thư Giãn", 79.000đ for women.
    dearer: String,
    item: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

fn key() -> String {
    Uuid::now_v7().to_string()
}

fn spa_today() -> NaiveDate {
    let vn = FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"));
    Utc::now().with_timezone(&vn).date_naive()
}

async fn service(h: &Harness, name: &str) -> Setup<String> {
    let id: Uuid = sqlx::query_scalar(
        "SELECT s.id FROM app.service s JOIN app.service_category c ON c.id = s.category_id
         WHERE c.name = 'Gội đầu dưỡng sinh' AND s.name = $1",
    )
    .bind(name)
    .fetch_one(&h.owner)
    .await?;
    Ok(id.to_string())
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Ngọc",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECHNICIAN_PHONE,
        "KTV Hoa",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let technician = session_for(&h, TECHNICIAN_PHONE).await;
    let shampoo = service(&h, "Cơ Bản – Dầu thảo dược").await?;
    let cheaper = service(&h, "Cơ Bản – Dầu cặp").await?;
    let dearer = service(&h, "Thư Giãn").await?;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let mai = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let customer = |name: &str, phone: &str| {
        let app = h.app.clone();
        let manager = manager.clone();
        let body = json!({"full_name": name, "phone": phone, "gender": "female"});
        async move { post_customer(&app, Some(&manager), Some(body)).await }
    };
    let ha = customer("Nguyễn Thị Hà", "0905111222").await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let binh = customer("Trần Thị Bình", "0905333444").await;
    assert_eq!(binh.status, StatusCode::CREATED, "{}", binh.body);
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu 10 buổi + tặng 2", "price": 1_500_000, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let sold = call_with_headers(
        &h.app,
        "POST",
        "/sales",
        Some(&manager),
        Some(json!({
            "customer_id": ha.body["id"], "closer_staff_id": lan.to_string(),
            "lines": [{"package_plan_id": plan.body["id"]}],
            "payments": [{"method": "cash", "amount": 1_500_000}],
        })),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    Ok(Spa {
        owner,
        manager,
        technician,
        ha: str_of(&ha.body["id"]),
        binh: str_of(&binh.body["id"]),
        lan: lan.to_string(),
        mai: mai.to_string(),
        shampoo,
        cheaper,
        dearer,
        item: str_of(&sold.body["customer_packages"][0]["items"][0]["id"]),
        h,
    })
}

impl Spa {
    fn one_off(&self, service: &str) -> Value {
        json!({"service_id": service, "source": "one_off", "staff_id": self.lan})
    }

    fn package_line(&self) -> Value {
        json!({"service_id": self.shampoo, "source": "package",
               "customer_package_item_id": self.item, "staff_id": self.lan})
    }

    async fn record(&self, lines: Vec<Value>, payments: Value) -> Reply {
        let v = call_with_headers(
            &self.h.app,
            "POST",
            "/visits",
            Some(&self.manager),
            Some(json!({"customer_id": self.ha, "lines": lines, "payments": payments})),
            &[("idempotency-key", &key())],
        )
        .await;
        assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
        v
    }

    /// `POST /visits/{id}/correct` with its own key and the given `If-Match` version.
    async fn correct(&self, who: &str, visit: &str, version: i64, body: Value) -> Reply {
        self.correct_with(who, visit, version, body, &key()).await
    }

    async fn correct_with(
        &self,
        who: &str,
        visit: &str,
        version: i64,
        body: Value,
        idempotency_key: &str,
    ) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "POST",
            &format!("/visits/{visit}/correct"),
            Some(who),
            Some(body),
            &[("idempotency-key", idempotency_key), ("If-Match", &etag)],
        )
        .await
    }

    async fn patch(&self, who: &str, visit: &str, version: i64, body: Value) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "PATCH",
            &format!("/visits/{visit}"),
            Some(who),
            Some(body),
            &[("If-Match", &etag)],
        )
        .await
    }

    async fn get(&self, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(&self.owner), None).await
    }

    async fn remaining(&self) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT sessions_remaining FROM app.customer_package_item WHERE id = $1::uuid",
        )
        .bind(&self.item)
        .fetch_one(&self.h.owner)
        .await
    }

    async fn count(&self, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.h.owner)
            .await
    }

    async fn exec(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }
}

// ------------------------------------------------------------------------------- direct edits

/// AC6b.1–6b.3: technician, service time and note change in place; money and sessions never.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_direct_edit_changes_technician_time_and_note_in_place(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(
            vec![s.package_line(), s.one_off(&s.shampoo)],
            json!([{"method": "cash", "amount": 49_000}]),
        )
        .await;
    let id = str_of(&v.body["id"]);
    let line = str_of(&v.body["lines"][0]["id"]);
    let midnight = format!("{}T00:00:00+07:00", spa_today());

    let edited = s
        .patch(
            &s.manager,
            &id,
            1,
            json!({"lines": [{"id": line, "staff_id": s.mai}], "check_in_at": midnight,
                   "note": "Khách thích lực nhẹ"}),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    assert_eq!(edited.body["id"], v.body["id"], "same visit");
    assert_eq!(edited.body["version"], 2);
    assert_eq!(edited.body["lines"][0]["staff_name"], "KTV Mai");
    assert_eq!(edited.body["lines"][1]["staff_name"], "KTV Lan");
    assert_eq!(edited.body["note"], "Khách thích lực nhẹ");
    let moved = chrono::DateTime::parse_from_rfc3339(&str_of(&edited.body["check_in_at"]))?;
    assert_eq!(moved, chrono::DateTime::parse_from_rfc3339(&midnight)?);
    assert_eq!(edited.body["total"], 49_000, "money untouched");
    assert_eq!(s.remaining().await?, 11, "sessions untouched");
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment").await?,
        2,
        "package sale + visit payment, nothing reversed"
    );
    assert_eq!(
        s.count("SELECT count(*) FROM app.audit_log WHERE action = 'visit.update'")
            .await?,
        1
    );

    // AC6b.2: the time stays on the visit's day and not ahead of now.
    let yesterday = format!("{}T23:00:00+07:00", spa_today() - chrono::Duration::days(1));
    let other_day = s
        .patch(&s.manager, &id, 2, json!({"check_in_at": yesterday}))
        .await;
    assert_eq!(
        code(&other_day),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(other_day.body["errors"][0]["field"], "check_in_at");
    assert_eq!(other_day.body["errors"][0]["code"], "not_visit_day");
    let later = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let future = s
        .patch(&s.manager, &id, 2, json!({"check_in_at": later}))
        .await;
    assert_eq!(
        code(&future),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_in_future")
        )
    );

    // AC6b.3: another device edited first.
    let stale = s
        .patch(
            &s.manager,
            &id,
            1,
            json!({"lines": [{"id": line, "staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(
        stale.status,
        StatusCode::PRECONDITION_FAILED,
        "{}",
        stale.body
    );
    let technician = s.patch(&s.technician, &id, 2, json!({"note": null})).await;
    assert_eq!(technician.status, StatusCode::FORBIDDEN);
    Ok(())
}

// --------------------------------------------------------------------------------- corrections

/// AC6b.4 + AC6b.12: a dearer service → the old visit is voided with "Sửa lượt: …", its payment
/// reversed, the new one recorded and linked; the daily report nets to the new amount.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn correcting_to_a_dearer_service_collects_the_difference(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(
            vec![s.one_off(&s.shampoo)],
            json!([{"method": "cash", "amount": 49_000}]),
        )
        .await;
    let old = str_of(&v.body["id"]);
    let r = s
        .correct(
            &s.manager,
            &old,
            1,
            json!({"reason": "  chọn nhầm dịch vụ ", "customer_id": s.ha,
                   "lines": [s.one_off(&s.dearer)],
                   "payments": [{"method": "cash", "amount": 79_000}]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let new = str_of(&r.body["visit"]["id"]);
    assert_ne!(new, old);
    assert_eq!(r.body["difference"], 30_000);
    assert_eq!(r.body["voided"]["status"], "voided");
    assert_eq!(
        r.body["voided"]["void_reason"],
        "Sửa lượt: chọn nhầm dịch vụ"
    );
    assert_eq!(r.body["voided"]["corrected_by_visit_id"], json!(new));
    assert_eq!(r.body["visit"]["status"], "completed");
    assert_eq!(r.body["visit"]["corrects_visit_id"], json!(old));
    assert_eq!(r.body["visit"]["total"], 79_000);
    assert_eq!(
        r.body["visit"]["check_in_at"], v.body["check_in_at"],
        "the service time is kept unless changed"
    );
    assert_eq!(
        r.headers.get("etag").and_then(|e| e.to_str().ok()),
        Some("\"1\"")
    );

    // Money is append-only: the old payment stays, reversed once; the new one is a fresh sale.
    let reversals = s
        .count("SELECT count(*) FROM app.payment WHERE kind = 'reversal' AND amount = 49000")
        .await?;
    assert_eq!(reversals, 1);
    let day = spa_today();
    let report = s.get(&format!("/reports/daily?date={day}")).await;
    assert_eq!(report.status, StatusCode::OK, "{}", report.body);
    let rev = &report.body["revenue"];
    // The package sold in the setup (1.500.000 cash) is on the same day.
    assert_eq!(rev["collected"]["total"], 1_500_000 + 49_000 + 79_000);
    assert_eq!(rev["voids"]["total"], 49_000);
    assert_eq!(rev["net"]["total"], 1_500_000 + 79_000);
    assert_eq!(report.body["counts"]["visits"], 1, "one real visit");
    assert_eq!(
        report.body["counts"]["voided_visits"], 0,
        "a correction is not a cancellation"
    );
    assert_eq!(report.body["counts"]["corrected_visits"], 1);
    let summary = s.get(&format!("/customers/{}/summary", s.ha)).await;
    assert_eq!(summary.body["visit_count"], 1, "{}", summary.body);
    assert_eq!(summary.body["voided_visit_count"], 0);
    assert_eq!(summary.body["net_spent"], 1_500_000 + 79_000);
    // The revenue book: the voided sale reads "Sửa lượt: …", the package invariant is untouched
    // (a one-off correction never turns into a "liệu trình" sale).
    let ledger = s.get("/reports/ledger").await;
    let rows = ledger.body["rows"].as_array().cloned().unwrap_or_default();
    assert!(
        rows.iter()
            .any(|r| r["entry"] == "void" && r["note"] == "Sửa lượt: chọn nhầm dịch vụ"),
        "{}",
        ledger.body
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "package").count(),
        1,
        "only the package sale of the setup"
    );

    // The day list shows both: the old one voided and pointing at the new one.
    let list = s.get("/visits").await;
    let rows = list.body.as_array().cloned().unwrap_or_default();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|v| v["id"] == json!(old)
        && v["status"] == "voided"
        && v["corrected_by_visit_id"] == json!(new)));

    for action in ["visit.void", "sale.void", "visit.create", "visit.correct"] {
        let n = s
            .count(&format!(
                "SELECT count(*) FROM app.audit_log WHERE action = '{action}'"
            ))
            .await?;
        assert!(n >= 1, "{action} audited");
    }
    assert_eq!(
        s.count(&format!(
            "SELECT count(*) FROM app.audit_log WHERE action = 'visit.correct'
             AND entity_id = '{old}' AND reason = 'Sửa lượt: chọn nhầm dịch vụ'
             AND after ->> 'difference' = '30000'"
        ))
        .await?,
        1
    );

    // AC6b.9: the old version cannot be corrected (nor voided, nor edited) again.
    let again = s
        .correct(
            &s.manager,
            &old,
            2,
            json!({"reason": "lần 2", "customer_id": s.ha, "lines": [s.one_off(&s.shampoo)],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    assert_eq!(
        code(&again),
        (StatusCode::CONFLICT, Some("visit.corrected"))
    );
    let patch = s.patch(&s.manager, &old, 2, json!({"note": null})).await;
    assert_eq!(
        code(&patch),
        (StatusCode::CONFLICT, Some("visit.corrected"))
    );
    Ok(())
}

/// AC6b.5: less money gives the difference back; a change of payment method alone keeps the
/// amount but still goes through void + record (payments are never updated).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn cheaper_or_same_amount_corrections_report_the_difference(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(
            vec![s.one_off(&s.shampoo)],
            json!([{"method": "cash", "amount": 49_000}]),
        )
        .await;
    let cheaper = s
        .correct(
            &s.manager,
            &str_of(&v.body["id"]),
            1,
            json!({"reason": "dầu cặp", "customer_id": s.ha, "lines": [s.one_off(&s.cheaper)],
                   "payments": [{"method": "cash", "amount": 39_000}]}),
        )
        .await;
    assert_eq!(cheaper.status, StatusCode::CREATED, "{}", cheaper.body);
    assert_eq!(cheaper.body["difference"], -10_000);

    // The newest version is corrected in turn: same amount, paid by transfer instead.
    let newest = str_of(&cheaper.body["visit"]["id"]);
    let same = s
        .correct(
            &s.manager,
            &newest,
            1,
            json!({"reason": "khách chuyển khoản", "customer_id": s.ha,
                   "lines": [s.one_off(&s.cheaper)],
                   "payments": [{"method": "bank_transfer", "amount": 39_000}]}),
        )
        .await;
    assert_eq!(same.status, StatusCode::CREATED, "{}", same.body);
    assert_eq!(same.body["difference"], 0);
    assert_eq!(same.body["visit"]["payments"][0]["method"], "bank_transfer");
    let report = s.get("/reports/daily").await;
    assert_eq!(
        report.body["revenue"]["net"]["bank_transfer"], 39_000,
        "{}",
        report.body
    );
    assert_eq!(report.body["revenue"]["net"]["cash"], 1_500_000);

    // A correction changing the customer moves the visit to the right person.
    let moved = s
        .correct(
            &s.manager,
            &str_of(&same.body["visit"]["id"]),
            1,
            json!({"reason": "nhầm khách", "customer_id": s.binh,
                   "lines": [s.one_off(&s.cheaper)],
                   "payments": [{"method": "bank_transfer", "amount": 39_000}]}),
        )
        .await;
    assert_eq!(moved.status, StatusCode::CREATED, "{}", moved.body);
    assert_eq!(moved.body["visit"]["customer_name"], "Trần Thị Bình");
    assert_eq!(moved.body["voided"]["customer_name"], "Nguyễn Thị Hà");
    Ok(())
}

/// AC6b.6: a package session becomes a one-off (session back, money taken) and back again.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_package_session_and_a_one_off_swap_places(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s.record(vec![s.package_line()], json!([])).await;
    assert_eq!(s.remaining().await?, 11);
    let to_one_off = s
        .correct(
            &s.manager,
            &str_of(&v.body["id"]),
            1,
            json!({"reason": "khách muốn trả tiền lẻ", "customer_id": s.ha,
                   "lines": [s.one_off(&s.shampoo)],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    assert_eq!(
        to_one_off.status,
        StatusCode::CREATED,
        "{}",
        to_one_off.body
    );
    assert_eq!(s.remaining().await?, 12, "the session is back");
    assert_eq!(to_one_off.body["difference"], 49_000);

    let back = s
        .correct(
            &s.manager,
            &str_of(&to_one_off.body["visit"]["id"]),
            1,
            json!({"reason": "trừ liệu trình", "customer_id": s.ha,
                   "lines": [s.package_line()], "payments": []}),
        )
        .await;
    assert_eq!(back.status, StatusCode::CREATED, "{}", back.body);
    assert_eq!(s.remaining().await?, 11);
    assert_eq!(back.body["difference"], -49_000);
    assert_eq!(back.body["visit"]["lines"][0]["sessions_left_after"], 11);
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment WHERE kind = 'reversal' AND amount = 49000")
            .await?,
        1
    );

    // A package line on the last session: the old visit's session is given back first, so the
    // correction can use it again.
    s.exec(&format!(
        "UPDATE app.customer_package_item SET sessions_remaining = 0 WHERE id = '{}'",
        s.item
    ))
    .await?;
    let staff_only = s
        .correct(
            &s.manager,
            &str_of(&back.body["visit"]["id"]),
            1,
            json!({"reason": "đổi KTV và thêm dịch vụ", "customer_id": s.ha,
                   "lines": [s.package_line(), s.one_off(&s.cheaper)],
                   "payments": [{"method": "cash", "amount": 39_000}]}),
        )
        .await;
    assert_eq!(
        staff_only.status,
        StatusCode::CREATED,
        "{}",
        staff_only.body
    );
    assert_eq!(s.remaining().await?, 0);
    Ok(())
}

/// AC6b.7 + AC6b.8: two devices correct the same visit at once → exactly one wins; a retry with
/// the same key replays the first correction.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn concurrent_corrections_one_wins_and_retries_replay(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s.record(vec![s.package_line()], json!([])).await;
    let id = str_of(&v.body["id"]);
    let body = |reason: &str| {
        json!({"reason": reason, "customer_id": s.ha, "lines": [s.one_off(&s.shampoo)],
               "payments": [{"method": "cash", "amount": 49_000}]})
    };
    let (a, b) = tokio::join!(
        s.correct(&s.manager, &id, 1, body("máy 1")),
        s.correct(&s.owner, &id, 1, body("máy 2"))
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::CREATED, StatusCode::CONFLICT],
        "{} / {}",
        a.body,
        b.body
    );
    let loser = if a.status == StatusCode::CONFLICT {
        &a
    } else {
        &b
    };
    assert_eq!(loser.body["code"], "visit.corrected");
    assert_eq!(s.remaining().await?, 12, "credited exactly once");
    assert_eq!(s.count("SELECT count(*) FROM app.visit").await?, 2);

    // Idempotent replay.
    let fresh = s.record(vec![s.package_line()], json!([])).await;
    let fresh_id = str_of(&fresh.body["id"]);
    let k = key();
    let first = s
        .correct_with(&s.manager, &fresh_id, 1, body("bấm 2 lần"), &k)
        .await;
    let retry = s
        .correct_with(&s.manager, &fresh_id, 1, body("bấm 2 lần"), &k)
        .await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(retry.status, StatusCode::CREATED, "{}", retry.body);
    assert_eq!(first.body["visit"]["id"], retry.body["visit"]["id"]);
    assert_eq!(s.count("SELECT count(*) FROM app.visit").await?, 4);
    assert_eq!(s.remaining().await?, 12);
    // The same key on another visit is not a retry.
    let other = s.record(vec![s.package_line()], json!([])).await;
    let reused = s
        .correct_with(
            &s.manager,
            &str_of(&other.body["id"]),
            1,
            body("bấm 2 lần"),
            &k,
        )
        .await;
    assert_eq!(
        code(&reused),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    Ok(())
}

/// AC6b.10 + AC6b.11: a closed day is the owner's, on that same day; technicians, other spas,
/// missing reason / If-Match are refused.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn closed_days_technicians_and_bad_requests(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(
            vec![s.one_off(&s.shampoo)],
            json!([{"method": "cash", "amount": 49_000}]),
        )
        .await;
    let id = str_of(&v.body["id"]);
    let body = json!({"reason": "nhầm giá", "customer_id": s.ha, "lines": [s.one_off(&s.dearer)],
                      "payments": [{"method": "cash", "amount": 79_000}]});

    let technician = s.correct(&s.technician, &id, 1, body.clone()).await;
    assert_eq!(technician.status, StatusCode::FORBIDDEN);
    let elsewhere = s
        .correct(&s.manager, &Uuid::now_v7().to_string(), 1, body.clone())
        .await;
    assert_eq!(elsewhere.status, StatusCode::NOT_FOUND);
    let mut no_reason = body.clone();
    no_reason["reason"] = json!("  ");
    let r = s.correct(&s.manager, &id, 1, no_reason).await;
    assert_eq!(
        code(&r),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(r.body["errors"][0]["field"], "reason");
    let no_if_match = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/visits/{id}/correct"),
        Some(&s.manager),
        Some(body.clone()),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(no_if_match.status, StatusCode::BAD_REQUEST);
    let stale = s.correct(&s.manager, &id, 7, body.clone()).await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        s.count("SELECT count(*) FROM app.visit WHERE status = 'voided'")
            .await?,
        0,
        "nothing changed"
    );

    // Yesterday's visit.
    s.exec("UPDATE app.visit SET business_date = business_date - 1, check_in_at = check_in_at - interval '1 day'")
        .await?;
    s.exec("UPDATE app.sale SET business_date = business_date - 1 WHERE id IN (SELECT sale_id FROM app.visit)")
        .await?;
    let manager = s.correct(&s.manager, &id, 1, body.clone()).await;
    assert_eq!(code(&manager), (StatusCode::FORBIDDEN, Some("day.locked")));
    // F17 (live test 2026-11-08): a bad body on a day the manager may not change is still
    // `403 day.locked` (it was 422 first), for a missing reason and for a body that does not parse.
    let mut no_reason = body.clone();
    no_reason["reason"] = json!("");
    for bad in [no_reason, json!({"lines": "not a list"})] {
        let r = s.correct(&s.manager, &id, 1, bad).await;
        assert_eq!(
            code(&r),
            (StatusCode::FORBIDDEN, Some("day.locked")),
            "{}",
            r.body
        );
    }
    let void = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/visits/{id}/void"),
        Some(&s.manager),
        Some(json!({"reason": ""})),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(
        code(&void),
        (StatusCode::FORBIDDEN, Some("day.locked")),
        "{}",
        void.body
    );
    let unknown = s
        .correct(&s.manager, &Uuid::now_v7().to_string(), 1, json!({}))
        .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND, "{}", unknown.body);
    let manager_patch = s.patch(&s.manager, &id, 1, json!({"note": null})).await;
    assert_eq!(
        code(&manager_patch),
        (StatusCode::FORBIDDEN, Some("day.locked"))
    );
    let owner = s.correct(&s.owner, &id, 1, body.clone()).await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    let yesterday = (spa_today() - chrono::Duration::days(1)).to_string();
    assert_eq!(
        owner.body["visit"]["business_date"],
        json!(yesterday),
        "AC6b.13"
    );
    // The reversal is booked on the day of the correction, like every void.
    assert_eq!(
        s.count(&format!(
            "SELECT count(*) FROM app.payment WHERE kind = 'reversal' AND business_date = '{}'",
            spa_today()
        ))
        .await?,
        1
    );
    // A correction never moves a visit to another day.
    let moved = s
        .correct(
            &s.owner,
            &str_of(&owner.body["visit"]["id"]),
            1,
            json!({"reason": "sai ngày", "customer_id": s.ha, "lines": [s.one_off(&s.dearer)],
                   "check_in_at": Utc::now().to_rfc3339(),
                   "payments": [{"method": "cash", "amount": 79_000}]}),
        )
        .await;
    assert_eq!(
        code(&moved),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    assert_eq!(moved.body["errors"][0]["field"], "check_in_at");
    Ok(())
}

/// The note travels server-side: absent = kept (re-encrypted for the new visit), null = cleared.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_session_note_is_kept_unless_changed(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = call_with_headers(
        &s.h.app,
        "POST",
        "/visits",
        Some(&s.manager),
        Some(json!({"customer_id": s.ha, "lines": [s.package_line()], "note": "Đau vai trái"})),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    let kept = s
        .correct(
            &s.manager,
            &str_of(&v.body["id"]),
            1,
            json!({"reason": "đổi dịch vụ", "customer_id": s.ha,
                   "lines": [s.one_off(&s.shampoo)],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    assert_eq!(kept.status, StatusCode::CREATED, "{}", kept.body);
    assert!(
        kept.body["visit"].get("note").is_none(),
        "never in the stored response"
    );
    let newest = str_of(&kept.body["visit"]["id"]);
    let read = s.get(&format!("/visits/{newest}")).await;
    assert_eq!(read.body["note"], "Đau vai trái");

    let cleared = s
        .correct(
            &s.manager,
            &newest,
            1,
            json!({"reason": "bỏ ghi chú", "customer_id": s.ha, "note": null,
                   "lines": [s.one_off(&s.shampoo)],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    assert_eq!(cleared.status, StatusCode::CREATED, "{}", cleared.body);
    let read = s
        .get(&format!("/visits/{}", str_of(&cleared.body["visit"]["id"])))
        .await;
    assert_eq!(read.body["note"], Value::Null);
    Ok(())
}

/// The calendar follows the correction: the appointment points at the new visit, and the voided
/// version is not shown as a second walk-in.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_appointment_moves_to_the_corrected_visit(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    sqlx::query(
        r#"UPDATE app.tenant SET settings = settings || '{"opening_hours": {"open": "00:00", "close": "23:45"}}'"#,
    )
    .execute(&s.h.owner)
    .await?;
    let booked = call_with_headers(
        &s.h.app,
        "POST",
        "/appointments",
        Some(&s.manager),
        Some(json!({"customer_id": s.ha, "service_ids": [s.shampoo],
                    "starts_at": format!("{}T00:00:00+07:00", spa_today() + chrono::Duration::days(1))})),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(booked.status, StatusCode::CREATED, "{}", booked.body);
    let appointment = str_of(&booked.body["id"]);
    // Booked for tomorrow, then moved onto today (a slot already over cannot be booked, F12).
    common::bookings_to_today(&s.h, &[&appointment]).await?;
    let done = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/appointments/{appointment}/complete"),
        Some(&s.manager),
        Some(json!({"lines": [s.package_line()]})),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    let old = str_of(&done.body["visit"]["id"]);
    let r = s
        .correct(
            &s.manager,
            &old,
            1,
            json!({"reason": "thêm dịch vụ", "customer_id": s.ha,
                   "lines": [s.package_line(), s.one_off(&s.cheaper)],
                   "payments": [{"method": "cash", "amount": 39_000}]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    let read = s.get(&format!("/appointments/{appointment}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.body);
    assert_eq!(read.body["visit_id"], r.body["visit"]["id"]);
    assert_eq!(read.body["status"], "done");
    assert_eq!(read.body["payment_state"], "paid");
    let calendar = s.get("/appointments").await;
    assert_eq!(
        calendar.body["visits"],
        json!([]),
        "neither version is a walk-in"
    );
    // The trigger still refuses any other change of a done appointment.
    let tamper = sqlx::query("UPDATE app.appointment SET visit_id = $1::uuid WHERE id = $2::uuid")
        .bind(&old)
        .bind(&appointment)
        .execute(&s.h.owner)
        .await;
    assert!(tamper.is_err(), "a done appointment cannot point back");
    Ok(())
}
