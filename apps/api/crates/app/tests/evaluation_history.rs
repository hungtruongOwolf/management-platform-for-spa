//! Evaluation history (EM 2026-11-08, spec staff-management §15): `GET /evaluations/history` for
//! one person across months — the rule of `GET /evaluations` (owner + manager, technicians and
//! former staff 403), another spa's or an unknown id 404, bounds 422 — and a technician's own
//! scorecard detail through `/me/staff` only. Then `GET /me/evaluations/history` (EM 2026-11-08,
//! follow-up): the session's own staff link only, former staff clamped to the months they worked.

mod common;

use axum::http::StatusCode;
use chrono::{Datelike, Months, NaiveDate, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{OWNER_PHONE, Reply, TestResult, add_member, call, harness, session_for};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::testing::app_pool;
use kernel::{Actor, TenantId, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";

fn this_month() -> NaiveDate {
    let today = Utc::now().with_timezone(&Ho_Chi_Minh).date_naive();
    today.with_day(1).unwrap_or(today)
}

fn ym(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

fn criteria() -> Value {
    json!([
        {"key": "technique", "max": 40, "level": "m2", "points": 32},
        {"key": "attitude", "max": 15, "level": "m1", "points": 15},
        {"key": "customer_feedback", "max": 15, "level": "m2", "points": 12},
        {"key": "diligence", "max": 10, "level": "m2", "points": 8},
        {"key": "hygiene", "max": 10, "level": "m2", "points": 8},
        {"key": "learning", "max": 10, "level": "m3", "points": 6}
    ])
}

struct Spa {
    h: common::Harness,
    owner: String,
    manager: String,
    tech: String,
    tenant: Uuid,
    lan: Uuid,
    hoa: Uuid,
}

async fn user_id(h: &common::Harness, phone: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
        .bind(format!("+84{}", &phone[1..]))
        .fetch_one(&h.owner)
        .await
}

/// A technician Lan (linked to the technician login) scored last month, with a praise day this
/// month; the manager's own profile Hoa (decision C: only the owner scores it).
async fn spa(pool: PgPool) -> Result<Spa, Box<dyn std::error::Error>> {
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
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "Lê Thị Lan", "Kỹ thuật viên").await?;
    let hoa = org::provisioning::add_staff(&h.owner, "vpyd", "Phạm Thị Hoa", "Quản lý").await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let (manager_user, tech_user) = (
        user_id(&h, MANAGER_PHONE).await?,
        user_id(&h, TECH_PHONE).await?,
    );
    for (staff, user) in [(lan, tech_user), (hoa, manager_user)] {
        sqlx::query("UPDATE app.staff SET user_id = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(tenant)
            .bind(staff)
            .bind(user)
            .execute(&h.owner)
            .await?;
    }
    let last = this_month() - Months::new(1);
    sqlx::query(
        "INSERT INTO app.staff_evaluation
           (tenant_id, staff_id, work_date, criteria, serious_issue, total_points, result_level,
            coaching_plan, comment, recorded_by)
         VALUES ($1, $2, $3, $4, false, 81, 'm2', NULL, 'Tay nghề tiến bộ', $5)",
    )
    .bind(tenant)
    .bind(lan)
    .bind(last)
    .bind(criteria())
    .bind(manager_user)
    .execute(&h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.staff_daily_review (tenant_id, staff_id, work_date, compliments, complaints, note, recorded_by)
         VALUES ($1, $2, $3, 2, 1, 'Khách khen', $4)",
    )
    .bind(tenant)
    .bind(lan)
    .bind(last + chrono::Duration::days(4))
    .bind(manager_user)
    .execute(&h.owner)
    .await?;
    Ok(Spa {
        h,
        owner,
        manager,
        tech,
        tenant,
        lan,
        hoa,
    })
}

impl Spa {
    async fn history(&self, session: Option<&str>, query: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            &format!("/evaluations/history{query}"),
            session,
            None,
        )
        .await
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owners_and_managers_read_a_persons_months(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let last = ym(this_month() - Months::new(1));
    for who in [&s.owner, &s.manager] {
        let r = s.history(Some(who), &format!("?staff_id={}", s.lan)).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        assert_eq!(r.body["full_name"], "Lê Thị Lan");
        let months = r.body["months"].as_array().cloned().unwrap_or_default();
        // The last 12 months by default, newest first (a profile with no start date counts as
        // employed throughout).
        assert_eq!(months.len(), 12);
        assert_eq!(months[0]["month"], ym(this_month()));
        let scored = months
            .iter()
            .find(|m| m["month"] == last.as_str())
            .cloned()
            .unwrap_or_default();
        assert_eq!(scored["evaluation"]["total_points"], 81);
        assert_eq!(scored["evaluation"]["result"], "m2");
        assert_eq!(scored["evaluation"]["comment"], "Tay nghề tiến bộ");
        assert_eq!(scored["evaluation"]["by_name"], "Quản lý Hoa");
        assert_eq!(
            scored["evaluation"]["criteria"].as_array().map(Vec::len),
            Some(6)
        );
        assert_eq!(scored["compliments"], 2);
        assert_eq!(scored["complaints"], 1);
        assert!(months[0]["evaluation"].is_null());
        assert_eq!(r.body["criteria"].as_array().map(Vec::len), Some(6));
    }
    // A bounded range.
    let one = s
        .history(
            Some(&s.owner),
            &format!("?staff_id={}&from={last}&to={last}", s.lan),
        )
        .await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.body);
    assert_eq!(one.body["months"].as_array().map(Vec::len), Some(1));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn decision_c_shows_who_may_rescore_but_both_may_read(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // The manager's own profile: readable by the manager, scored only by the owner.
    let mine = s
        .history(Some(&s.manager), &format!("?staff_id={}", s.hoa))
        .await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.body);
    assert_eq!(mine.body["can_record"], false);
    let owner = s
        .history(Some(&s.owner), &format!("?staff_id={}", s.hoa))
        .await;
    assert_eq!(owner.body["can_record"], true);
    let lan = s
        .history(Some(&s.manager), &format!("?staff_id={}", s.lan))
        .await;
    assert_eq!(lan.body["can_record"], true);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technicians_former_staff_and_other_spas_cannot_read_it(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // A technician: not even their own history here (their months are under /me/staff).
    for id in [s.lan, s.hoa] {
        let r = s.history(Some(&s.tech), &format!("?staff_id={id}")).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    }
    assert_eq!(
        s.history(None, &format!("?staff_id={}", s.lan))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // A manager whose profile has left keeps only the former-staff permissions (ADR-0014).
    let former =
        kernel::permission::effective_permissions(SystemRole::Manager.default_permissions(), true);
    let actor = Actor::new(UserId::new(), TenantId::from_uuid(s.tenant), former, None);
    let app = app_pool(&s.h.owner).await?;
    let denied = hr::evaluation::history(&app, &actor, Some(s.lan), None, None).await;
    assert!(
        matches!(denied, Err(kernel::AppError::Forbidden { .. })),
        "{denied:?}"
    );
    // Another spa's owner: our people are not found, theirs are.
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
    let theirs =
        org::provisioning::add_staff(&s.h.owner, "khac", "KTV Khác", "Kỹ thuật viên").await?;
    assert_eq!(
        s.history(Some(&other), &format!("?staff_id={}", s.lan))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.history(Some(&other), &format!("?staff_id={theirs}"))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        s.history(Some(&s.owner), &format!("?staff_id={theirs}"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.history(Some(&s.owner), &format!("?staff_id={}", Uuid::now_v7()))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn bad_requests_are_refused(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let id = s.lan;
    assert_eq!(
        s.history(Some(&s.owner), "").await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.history(
            Some(&s.owner),
            &format!("?staff_id={id}&from=2026-10&to=2026-09")
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.history(
            Some(&s.owner),
            &format!("?staff_id={id}&from=2023-01&to=2026-01")
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.history(Some(&s.owner), &format!("?staff_id={id}&from=2026-13"))
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        s.history(Some(&s.owner), &format!("?staff_id={id}&userId=x"))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    Ok(())
}

/// A technician reads their own scorecard detail (criteria, comment, scorer) for a past month
/// through `/me/staff` — never anyone else's: the route takes no person parameter.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_reads_only_their_own_scorecard(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let last = ym(this_month() - Months::new(1));
    let me = call(
        &s.h.app,
        "GET",
        &format!("/me/staff?month={last}"),
        Some(&s.tech),
        None,
    )
    .await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    let text = me.body.to_string();
    assert!(text.contains("Tay nghề tiến bộ"), "{text}");
    assert!(text.contains("\"technique\""), "{text}");
    let other = call(
        &s.h.app,
        "GET",
        &format!("/me/staff?month={last}&staff_id={}", s.hoa),
        Some(&s.tech),
        None,
    )
    .await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST);
    Ok(())
}

// ---------------------------------------------------------------- GET /me/evaluations/history

async fn mine(s: &Spa, session: Option<&str>, query: &str) -> Reply {
    call(
        &s.h.app,
        "GET",
        &format!("/me/evaluations/history{query}"),
        session,
        None,
    )
    .await
}

fn months_of(r: &Reply) -> Vec<String> {
    r.body["data"]["months"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["month"].as_str().map(str::to_owned))
        .collect()
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_technician_reads_their_own_months_only(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let last = ym(this_month() - Months::new(1));
    let r = mine(&s, Some(&s.tech), "").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["linked"], true);
    assert!(r.body.get("left_on").is_none());
    let data = &r.body["data"];
    assert_eq!(data["full_name"], "Lê Thị Lan");
    assert_eq!(months_of(&r).len(), 12);
    assert_eq!(months_of(&r)[0], ym(this_month()));
    let scored = data["months"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["month"] == last.as_str())
        .cloned()
        .unwrap_or_default();
    assert_eq!(scored["evaluation"]["comment"], "Tay nghề tiến bộ");
    assert_eq!(scored["compliments"], 2);
    // Same shape as the managers' history, without the writer-only keys.
    assert!(data.get("can_record").is_none() && data.get("staff_id").is_none());
    // No way to ask for someone else; same range rules.
    let other = mine(&s, Some(&s.tech), &format!("?staff_id={}", s.hoa)).await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST);
    let one = mine(&s, Some(&s.tech), &format!("?from={last}&to={last}")).await;
    assert_eq!(months_of(&one), [last]);
    assert_eq!(
        mine(&s, Some(&s.tech), "?from=2026-10&to=2026-09")
            .await
            .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(mine(&s, None, "").await.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

/// ADR-0014: a former staff member is not refused a range; the window ends at the month they
/// left, a later `to` is clamped to it, and months they were not employed in are left out.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn former_staff_see_only_the_months_they_worked(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let started = this_month() - Months::new(4);
    let left = this_month() - Months::new(1) + chrono::Duration::days(14);
    sqlx::query(
        "UPDATE app.staff_employment SET started_on = $3, ended_on = $4, end_reason = 'Chuyển về quê'
         WHERE tenant_id = $1 AND staff_id = $2 AND ended_on IS NULL",
    )
    .bind(s.tenant)
    .bind(s.lan)
    .bind(started)
    .bind(left)
    .execute(&s.h.owner)
    .await?;
    sqlx::query(
        "UPDATE app.staff SET status = 'left', joined_on = $3, left_on = $4 WHERE tenant_id = $1 AND id = $2",
    )
    .bind(s.tenant)
    .bind(s.lan)
    .bind(started)
    .bind(left)
    .execute(&s.h.owner)
    .await?;
    let expected: Vec<String> = (1..=4).map(|n| ym(this_month() - Months::new(n))).collect();
    let r = mine(&s, Some(&s.tech), "").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["left_on"], left.to_string());
    assert_eq!(months_of(&r), expected, "the months worked, newest first");
    // A range reaching past the end: clamped, not refused.
    let past_end = mine(&s, Some(&s.tech), &format!("?to={}", ym(this_month()))).await;
    assert_eq!(past_end.status, StatusCode::OK, "{}", past_end.body);
    assert_eq!(months_of(&past_end), expected);
    // A range wholly before they started: empty, not refused.
    let before = this_month() - Months::new(8);
    let none = mine(
        &s,
        Some(&s.tech),
        &format!("?from={}&to={}", ym(before), ym(before)),
    )
    .await;
    assert_eq!(none.status, StatusCode::OK, "{}", none.body);
    assert!(months_of(&none).is_empty());
    // Still no managers' history for them.
    assert_eq!(
        s.history(Some(&s.tech), &format!("?staff_id={}", s.lan))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owners_and_managers_get_their_own_link_or_nothing(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // The manager's login is linked to Hoa: her months, nobody else's.
    let manager = mine(&s, Some(&s.manager), "").await;
    assert_eq!(manager.status, StatusCode::OK, "{}", manager.body);
    assert_eq!(manager.body["data"]["full_name"], "Phạm Thị Hoa");
    assert!(!manager.body.to_string().contains("Tay nghề tiến bộ"));
    // The owner has no staff profile: the documented empty answer, as /me/staff.
    let owner = mine(&s, Some(&s.owner), "").await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(owner.body["linked"], false);
    assert!(owner.body.get("data").is_none());
    // Another spa's owner: their own (empty) answer, never this spa's people.
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
    let theirs = mine(&s, Some(&other), "").await;
    assert_eq!(theirs.status, StatusCode::OK, "{}", theirs.body);
    assert_eq!(theirs.body["linked"], false);
    assert!(!theirs.body.to_string().contains("Lê Thị Lan"));
    Ok(())
}
