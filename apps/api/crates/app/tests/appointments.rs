//! Round 4: appointments (spec appointments.md AC9.1–9.8), owner back-dating (AC5.1–5.2) and
//! discounts on visits (AC8.1–8.2).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{Duration, FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    /// Female customer with a "10 + 2" shampoo package.
    ha: String,
    ha_item: String,
    /// Male customer, no package.
    duc: String,
    lan: String,
    tuan: String,
    shampoo: String,
    /// 90 minutes, Nữ 349.000đ / Nam 399.000đ.
    dai_thong_lac: String,
}

fn id_of(reply: &Reply) -> String {
    reply.body["id"].as_str().unwrap_or_default().to_owned()
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

fn service_id(services: &Value, name: &str) -> String {
    services
        .as_array()
        .and_then(|l| l.iter().find(|s| s["name"] == name))
        .and_then(|s| s["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn vn() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"))
}

fn spa_today() -> NaiveDate {
    Utc::now().with_timezone(&vn()).date_naive()
}

/// `HH:MM` on the spa's day `today + days`, as RFC 3339.
fn at(days: i64, hm: &str) -> String {
    format!("{}T{hm}:00+07:00", spa_today() + Duration::days(days))
}

fn key() -> String {
    Uuid::now_v7().to_string()
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
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let services = call(&h.app, "GET", "/services", Some(&owner), None)
        .await
        .body;
    let shampoo = service_id(&services, "Cơ Bản – Dầu thảo dược");
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu dưỡng sinh 10 buổi + tặng 2", "price": 1_500_000, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let mut customers = Vec::new();
    for (name, phone, gender) in [
        ("Nguyễn Thị Hà", "0905111222", "female"),
        ("Trần Văn Đức", "0905333444", "male"),
    ] {
        let c = call(
            &h.app,
            "POST",
            "/customers",
            Some(&manager),
            Some(json!({"full_name": name, "phone": phone, "gender": gender})),
        )
        .await;
        assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
        customers.push(id_of(&c));
    }
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let tuan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Tuấn", "Kỹ thuật viên").await?;
    // Not a technician: does not add a column or capacity.
    org::provisioning::add_staff(&h.owner, "vpyd", "Chị Ngọc", "Quản lý").await?;
    let sold = call_with_headers(
        &h.app,
        "POST",
        "/sales",
        Some(&manager),
        Some(json!({
            "customer_id": customers[0], "closer_staff_id": lan,
            "lines": [{"package_plan_id": id_of(&plan)}],
            "payments": [{"method": "cash", "amount": 1_500_000}],
        })),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    let ha_item = sold.body["customer_packages"][0]["items"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    Ok(Spa {
        owner,
        manager,
        ha: customers[0].clone(),
        ha_item,
        duc: customers[1].clone(),
        lan: lan.to_string(),
        tuan: tuan.to_string(),
        dai_thong_lac: service_id(&services, "Đại Thông Lạc"),
        shampoo,
        h,
    })
}

impl Spa {
    async fn post(&self, who: &str, path: &str, body: Value) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(who),
            Some(body),
            &[("idempotency-key", &key())],
        )
        .await
    }

    async fn book(&self, who: &str, body: Value) -> Reply {
        self.post(who, "/appointments", body).await
    }

    async fn book_ha(&self, who: &str, starts_at: &str, staff: Option<&str>) -> Reply {
        self.book(
            who,
            json!({"customer_id": self.ha, "service_ids": [self.dai_thong_lac], "staff_id": staff,
                   "starts_at": starts_at}),
        )
        .await
    }

    /// Opening hours all day long, so "today" bookings work whatever time the test runs.
    async fn open_all_day(&self) -> Setup<()> {
        sqlx::query(
            r#"UPDATE app.tenant SET settings = settings || '{"opening_hours": {"open": "00:00", "close": "23:45"}}'"#,
        )
        .execute(&self.h.owner)
        .await?;
        Ok(())
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn bookings_respect_technicians_and_the_grid(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // AC9.1: 90-minute service → 10:00–11:30 on Lan's column.
    let first = s.book_ha(&s.manager, &at(1, "10:00"), Some(&s.lan)).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_eq!(first.body["status"], "booked");
    assert_eq!(first.body["duration_min"], 90);
    assert_eq!(first.body["staff_name"], "KTV Lan");
    assert_eq!(first.body["payment_state"], "unpaid");

    // AC9.2: Lan is busy until 11:30, free from then on.
    let busy = s.book_ha(&s.manager, &at(1, "11:00"), Some(&s.lan)).await;
    assert_eq!(
        code(&busy),
        (StatusCode::CONFLICT, Some("appointment.staff_busy"))
    );
    assert_eq!(busy.body["existing_id"], first.body["id"]);
    let later = s.book_ha(&s.manager, &at(1, "11:30"), Some(&s.lan)).await;
    assert_eq!(later.status, StatusCode::CREATED, "{}", later.body);

    // Columns: the two technicians ("Quản lý" is not one). No capacity derived from them
    // (round 5, AC10.4): with no owner setting, overlapping bookings are all accepted.
    let calendar = call(
        &s.h.app,
        "GET",
        &format!("/appointments?from={}", spa_today() + Duration::days(1)),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(calendar.status, StatusCode::OK, "{}", calendar.body);
    assert_eq!(
        calendar.body["technicians"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(calendar.body["max_per_slot"], Value::Null);
    assert!(calendar.body.get("capacity").is_none());
    assert_eq!(calendar.body["opens_at"], "09:00");
    assert_eq!(calendar.body["slot_minutes"], 15);
    assert_eq!(calendar.body["appointments"][0]["phase"], "waiting");
    for (hm, staff) in [("10:30", None), ("11:00", None), ("11:15", Some(&s.tuan))] {
        let more = s
            .book_ha(&s.manager, &at(1, hm), staff.map(String::as_str))
            .await;
        assert_eq!(more.status, StatusCode::CREATED, "{hm}: {}", more.body);
    }

    // AC9.4: on the 15-minute grid, inside 09:00–21:00.
    let off_grid = s.book_ha(&s.manager, &at(1, "13:07"), None).await;
    assert_eq!(
        code(&off_grid),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed"))
    );
    for early_or_late in ["08:30", "20:00"] {
        let r = s.book_ha(&s.manager, &at(1, early_or_late), None).await;
        assert_eq!(
            code(&r),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Some("appointment.outside_opening_hours")
            ),
            "{early_or_late}"
        );
    }
    let too_far = s.book_ha(&s.manager, &at(120, "10:00"), None).await;
    assert_eq!(too_far.status, StatusCode::UNPROCESSABLE_ENTITY);

    // A newcomer whose phone already has a record is pointed at it.
    let taken = s
        .book(
            &s.manager,
            json!({"guest": {"full_name": "Anh Đức", "phone": "0905 333 444"},
                   "service_ids": [s.shampoo], "starts_at": at(2, "10:00")}),
        )
        .await;
    assert_eq!(
        code(&taken),
        (StatusCode::CONFLICT, Some("customer.phone_taken"))
    );
    assert_eq!(taken.body["existing_id"], json!(s.duc));
    let guest = s
        .book(
            &s.manager,
            json!({"guest": {"full_name": "Chị Mai", "phone": "0905 999 000"},
                   "service_ids": [s.shampoo], "starts_at": at(2, "10:00"), "note": "Khách hỏi giờ"}),
        )
        .await;
    assert_eq!(guest.status, StatusCode::CREATED, "{}", guest.body);
    assert_eq!(guest.body["is_guest"], true);
    assert_eq!(guest.body["customer_phone"], "+84905999000");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn two_racing_bookings_of_one_technician_cannot_both_win(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (two, half_past) = (at(1, "14:00"), at(1, "14:30"));
    let (a, b) = tokio::join!(
        s.book_ha(&s.manager, &two, Some(&s.lan)),
        s.book_ha(&s.manager, &half_past, Some(&s.lan)),
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
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_appointment_becomes_a_visit_through_the_visit_rules(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.open_all_day().await?;
    let booked = s
        .book(
            &s.manager,
            json!({"customer_id": s.ha, "service_ids": [s.shampoo], "starts_at": at(0, "00:00")}),
        )
        .await;
    assert_eq!(booked.status, StatusCode::CREATED, "{}", booked.body);
    let id = id_of(&booked);

    // AC9.5: arrive with a technician, then complete with the cart.
    let arrived = call(
        &s.h.app,
        "POST",
        &format!("/appointments/{id}/arrive"),
        Some(&s.manager),
        Some(json!({"staff_id": s.lan})),
    )
    .await;
    assert_eq!(arrived.status, StatusCode::OK, "{}", arrived.body);
    assert_eq!(arrived.body["status"], "arrived");
    assert!(arrived.body["arrived_at"].is_string());
    // AC10.3: arrived = in service.
    assert_eq!(booked.body["phase"], "waiting");
    assert_eq!(arrived.body["phase"], "in_service");

    let cart = json!({"lines": [{"service_id": s.shampoo, "source": "package",
                                 "customer_package_item_id": s.ha_item, "staff_id": s.lan}]});
    let k = key();
    let path = format!("/appointments/{id}/complete");
    let headers = [("idempotency-key", k.as_str())];
    let done = call_with_headers(
        &s.h.app,
        "POST",
        &path,
        Some(&s.manager),
        Some(cart.clone()),
        &headers,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    assert_eq!(done.body["appointment"]["status"], "done");
    assert_eq!(done.body["appointment"]["payment_state"], "no_charge");
    assert_eq!(
        done.body["appointment"]["visit_id"],
        done.body["visit"]["id"]
    );
    assert_eq!(done.body["visit"]["lines"][0]["sessions_left_after"], 11);
    // AC10.3: the visit started just now (on arrival) and lasts 30 minutes: still in service.
    assert_eq!(done.body["appointment"]["phase"], "in_service");
    // The visit happened when the customer arrived, not when it was typed.
    assert_eq!(
        done.body["visit"]["check_in_at"],
        arrived.body["arrived_at"]
    );

    // Replay: same answer, no second session taken.
    let replay = call_with_headers(
        &s.h.app,
        "POST",
        &path,
        Some(&s.manager),
        Some(cart.clone()),
        &headers,
    )
    .await;
    assert_eq!(replay.body, done.body);
    let packages = call(
        &s.h.app,
        "GET",
        &format!("/customers/{}/packages", s.ha),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(packages.body[0]["items"][0]["sessions_remaining"], 11);
    // AC9.6: done is final.
    let again = s.post(&s.manager, &path, cart).await;
    assert_eq!(
        code(&again),
        (StatusCode::CONFLICT, Some("appointment.closed"))
    );

    // A paid one-off appointment shows its payment.
    let paid = s.book_ha(&s.manager, &at(0, "02:00"), Some(&s.tuan)).await;
    let paid_id = id_of(&paid);
    let done = s
        .post(
            &s.manager,
            &format!("/appointments/{paid_id}/complete"),
            json!({"lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.tuan}],
                   "payments": [{"method": "bank_transfer", "amount": 349_000}]}),
        )
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    assert_eq!(done.body["appointment"]["payment_state"], "paid");
    assert_eq!(done.body["appointment"]["paid_amount"], 349_000);

    // AC9.6: a newcomer needs a customer record first; linking it unlocks completion.
    let guest = s
        .book(
            &s.manager,
            json!({"guest": {"full_name": "Chị Mai", "phone": "0905 999 000"},
                   "service_ids": [s.dai_thong_lac], "starts_at": at(0, "04:00")}),
        )
        .await;
    let gid = id_of(&guest);
    let body = json!({"lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.lan,
                                 "price_variant": "female"}],
                      "payments": [{"method": "cash", "amount": 349_000}]});
    let early = s
        .post(
            &s.manager,
            &format!("/appointments/{gid}/complete"),
            body.clone(),
        )
        .await;
    assert_eq!(
        code(&early),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("appointment.customer_required")
        )
    );
    let mai = call(
        &s.h.app,
        "POST",
        "/customers",
        Some(&s.manager),
        Some(json!({"full_name": "Chị Mai", "phone": "0905999000", "gender": "female"})),
    )
    .await;
    let linked = call_with_headers(
        &s.h.app,
        "PATCH",
        &format!("/appointments/{gid}"),
        Some(&s.manager),
        Some(json!({"customer_id": id_of(&mai)})),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.body);
    assert_eq!(linked.body["is_guest"], false);
    let mut body = body;
    body["lines"][0]["price_variant"] = Value::Null;
    let done = s
        .post(&s.manager, &format!("/appointments/{gid}/complete"), body)
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    Ok(())
}

/// `GET /appointments` for `day` (today + `days`) as `who`.
async fn calendar_of(s: &Spa, who: &str, days: i64) -> Reply {
    call(
        &s.h.app,
        "GET",
        &format!("/appointments?from={}", spa_today() + Duration::days(days)),
        Some(who),
        None,
    )
    .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn walk_in_visits_share_the_timeline_with_bookings(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.open_all_day().await?;
    // AC10.1: a walk-in recorded now (90 minutes, Tuấn) is on today's calendar, in service.
    let walk_in = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.duc,
                   "lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.tuan}],
                   "payments": [{"method": "cash", "amount": 399_000}]}),
        )
        .await;
    assert_eq!(walk_in.status, StatusCode::CREATED, "{}", walk_in.body);
    // A booking completed into a visit is that booking, not a second walk-in.
    let booked = s
        .book(
            &s.manager,
            json!({"customer_id": s.ha, "service_ids": [s.shampoo], "starts_at": at(0, "00:00")}),
        )
        .await;
    let completed = s
        .post(
            &s.manager,
            &format!("/appointments/{}/complete", id_of(&booked)),
            json!({"lines": [{"service_id": s.shampoo, "source": "package",
                              "customer_package_item_id": s.ha_item, "staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.body);

    let today = calendar_of(&s, &s.manager, 0).await;
    assert_eq!(today.status, StatusCode::OK, "{}", today.body);
    let visits = today.body["visits"].as_array().cloned().unwrap_or_default();
    assert_eq!(visits.len(), 1, "{}", today.body["visits"]);
    let v = &visits[0];
    assert_eq!(v["id"], walk_in.body["id"]);
    assert_eq!(v["phase"], "in_service");
    assert_eq!(v["duration_min"], 90);
    assert_eq!(v["staff_name"], "KTV Tuấn");
    assert_eq!(v["customer_name"], "Trần Văn Đức");
    assert_eq!(v["payment_state"], "paid");
    assert_eq!(v["paid_amount"], 399_000);
    assert_eq!(v["services"][0]["service_name"], "Đại Thông Lạc");
    assert!(v.get("note").is_none() && v.get("customer_phone").is_none());

    // The owner back-dates a walk-in to yesterday 10:00: long over, so done.
    let yesterday = s
        .post(
            &s.owner,
            "/visits",
            json!({"customer_id": s.duc, "check_in_at": at(-1, "10:00"),
                   "lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.tuan}],
                   "payments": [{"method": "cash", "amount": 399_000}]}),
        )
        .await;
    assert_eq!(yesterday.status, StatusCode::CREATED, "{}", yesterday.body);
    let past = calendar_of(&s, &s.owner, -1).await;
    assert_eq!(past.body["visits"][0]["phase"], "done", "{}", past.body);
    let shown = past.body["visits"][0]["starts_at"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(shown)?,
        chrono::DateTime::parse_from_rfc3339(&at(-1, "10:00"))?
    );

    // AC10.2: a voided walk-in stays, marked voided.
    let void = s
        .post(
            &s.manager,
            &format!("/visits/{}/void", id_of(&walk_in)),
            json!({"reason": "Ghi nhầm"}),
        )
        .await;
    assert!(void.status.is_success(), "{}", void.body);
    let today = calendar_of(&s, &s.manager, 0).await;
    assert_eq!(today.body["visits"][0]["phase"], "voided");
    assert_eq!(today.body["visits"][0]["payment_state"], "voided");

    // AC10.2: without visit.read the calendar carries no visits.
    sqlx::query(
        "DELETE FROM app.role_permission WHERE permission = 'visit.read'
           AND role_id IN (SELECT id FROM app.role WHERE key = 'manager')",
    )
    .execute(&s.h.owner)
    .await?;
    let manager = session_for(&s.h, MANAGER_PHONE).await;
    let blind = calendar_of(&s, &manager, 0).await;
    assert_eq!(blind.status, StatusCode::OK, "{}", blind.body);
    assert_eq!(blind.body["visits"], json!([]));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_sets_the_advisory_max_per_slot(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let put = |who: &str, body: Value| {
        let who = who.to_owned();
        let app = s.h.app.clone();
        async move { call(&app, "PUT", "/appointment-settings", Some(&who), Some(body)).await }
    };
    // AC10.5: managers cannot; bad values are refused.
    let manager = put(&s.manager, json!({"max_per_slot": 2})).await;
    assert_eq!(manager.status, StatusCode::FORBIDDEN);
    for bad in [
        json!({"max_per_slot": 0}),
        json!({"max_per_slot": 201}),
        json!({}),
    ] {
        let r = put(&s.owner, bad.clone()).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {}",
            r.body
        );
    }
    let text = put(&s.owner, json!({"max_per_slot": "abc"})).await;
    assert!(text.status.is_client_error(), "{}", text.body);
    let anonymous = call(
        &s.h.app,
        "PUT",
        "/appointment-settings",
        None,
        Some(json!({"max_per_slot": 2})),
    )
    .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

    // AC10.4: the owner sets 2; the calendar shows it; a third overlapping booking still goes in.
    let set = put(&s.owner, json!({"max_per_slot": 2})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.body);
    assert_eq!(set.body["max_per_slot"], 2);
    for hm in ["10:00", "10:00", "10:15"] {
        let r = s.book_ha(&s.manager, &at(1, hm), None).await;
        assert_eq!(r.status, StatusCode::CREATED, "{hm}: {}", r.body);
    }
    assert_eq!(calendar_of(&s, &s.manager, 1).await.body["max_per_slot"], 2);
    let cleared = put(&s.owner, json!({"max_per_slot": null})).await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    assert_eq!(
        calendar_of(&s, &s.manager, 1).await.body["max_per_slot"],
        Value::Null
    );
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE action = 'appointment.settings'",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(audited, 2);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_family_is_booked_at_once_or_not_at_all(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let group = |people: Value| json!({"starts_at": at(1, "10:00"), "note": "Gia đình chị Hà", "people": people});
    let family = group(json!([
        {"customer_id": s.ha, "service_ids": [s.dai_thong_lac], "staff_id": s.lan},
        {"customer_id": s.duc, "service_ids": [s.shampoo], "staff_id": s.tuan},
        {"guest": {"full_name": "Bé Na", "phone": "0905 777 888"}, "service_ids": [s.shampoo],
         "duration_min": 60},
    ]));
    // AC10.6: three appointments, one group, each with its own services / technician / length.
    let k = key();
    let headers = [("idempotency-key", k.as_str())];
    let booked = call_with_headers(
        &s.h.app,
        "POST",
        "/appointments/group",
        Some(&s.manager),
        Some(family.clone()),
        &headers,
    )
    .await;
    assert_eq!(booked.status, StatusCode::CREATED, "{}", booked.body);
    let list = booked.body["appointments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(list.len(), 3);
    for a in &list {
        assert_eq!(a["group_id"], booked.body["group_id"]);
        assert_eq!(a["note"], "Gia đình chị Hà");
        assert_eq!(a["status"], "booked");
    }
    assert_eq!(
        list.iter()
            .map(|a| a["duration_min"].clone())
            .collect::<Vec<_>>(),
        [json!(90), json!(30), json!(60)]
    );
    assert_eq!(list[1]["staff_name"], "KTV Tuấn");
    assert_eq!(list[2]["is_guest"], true);
    // Replay: the same answer, nothing added.
    let replay = call_with_headers(
        &s.h.app,
        "POST",
        "/appointments/group",
        Some(&s.manager),
        Some(family),
        &headers,
    )
    .await;
    assert_eq!(replay.body, booked.body);
    let count = |s: &Spa| {
        let pool = s.h.owner.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.appointment")
                .fetch_one(&pool)
                .await
        }
    };
    assert_eq!(count(&s).await?, 3);

    // AC10.7: one person clashing with an existing booking → nobody is saved.
    let clash = s
        .post(
            &s.manager,
            "/appointments/group",
            json!({"starts_at": at(1, "10:30"), "people": [
                {"customer_id": s.duc, "service_ids": [s.shampoo]},
                {"customer_id": s.ha, "service_ids": [s.shampoo], "staff_id": s.lan},
            ]}),
        )
        .await;
    assert_eq!(
        code(&clash),
        (StatusCode::CONFLICT, Some("appointment.staff_busy"))
    );
    assert_eq!(clash.body["existing_id"], list[0]["id"]);
    // Two people of the group on one technician at once.
    let same_staff = s
        .post(
            &s.manager,
            "/appointments/group",
            json!({"starts_at": at(1, "14:00"), "people": [
                {"customer_id": s.duc, "service_ids": [s.shampoo], "staff_id": s.tuan},
                {"customer_id": s.ha, "service_ids": [s.shampoo], "staff_id": s.tuan},
            ]}),
        )
        .await;
    assert_eq!(
        code(&same_staff),
        (StatusCode::CONFLICT, Some("appointment.staff_busy"))
    );
    // Size, duplicates, a bad person: 422 naming the person; never a partial group.
    let one = s
        .post(
            &s.manager,
            "/appointments/group",
            group(json!([{"customer_id": s.ha, "service_ids": [s.shampoo]}])),
        )
        .await;
    assert_eq!(one.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(one.body["errors"][0]["field"], "people", "{}", one.body);
    let seven: Vec<Value> = (0..7)
        .map(|i| {
            json!({"guest": {"full_name": format!("Khách {i}"), "phone": format!("090577700{i}")},
                        "service_ids": [s.shampoo]})
        })
        .collect();
    let too_many = s
        .post(&s.manager, "/appointments/group", group(json!(seven)))
        .await;
    assert_eq!(too_many.status, StatusCode::UNPROCESSABLE_ENTITY);
    let twice = s
        .post(
            &s.manager,
            "/appointments/group",
            json!({"starts_at": at(2, "10:00"), "people": [
                {"customer_id": s.ha, "service_ids": [s.shampoo]},
                {"customer_id": s.ha, "service_ids": [s.shampoo]},
            ]}),
        )
        .await;
    assert_eq!(twice.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(twice.body["errors"][0]["field"], "people[1].customer_id");
    let bad_second = s
        .post(
            &s.manager,
            "/appointments/group",
            json!({"starts_at": at(2, "10:00"), "people": [
                {"customer_id": s.ha, "service_ids": [s.shampoo]},
                {"guest": {"full_name": "Bé Bi", "phone": "abc"}, "service_ids": [s.shampoo]},
            ]}),
        )
        .await;
    assert_eq!(bad_second.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        bad_second.body["errors"][0]["field"],
        "people[1].guest.phone"
    );
    let no_services = s
        .post(
            &s.manager,
            "/appointments/group",
            json!({"starts_at": at(2, "10:00"), "people": [
                {"customer_id": s.ha, "service_ids": [s.shampoo]},
                {"customer_id": s.duc, "service_ids": []},
            ]}),
        )
        .await;
    assert_eq!(no_services.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        no_services.body["errors"][0]["field"],
        "people[1].service_ids"
    );
    assert_eq!(count(&s).await?, 3, "no partial group was saved");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn cancel_and_no_show_need_a_reason_and_free_the_slot(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let a = s.book_ha(&s.manager, &at(1, "10:00"), Some(&s.lan)).await;
    let id = id_of(&a);
    let no_reason = call(
        &s.h.app,
        "POST",
        &format!("/appointments/{id}/cancel"),
        Some(&s.manager),
        Some(json!({})),
    )
    .await;
    assert_eq!(no_reason.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Not yet started: it cannot be a no-show.
    let early = call(
        &s.h.app,
        "POST",
        &format!("/appointments/{id}/no-show"),
        Some(&s.manager),
        Some(json!({"reason": "Không đến"})),
    )
    .await;
    assert_eq!(
        code(&early),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("appointment.not_started")
        )
    );
    let cancelled = call(
        &s.h.app,
        "POST",
        &format!("/appointments/{id}/cancel"),
        Some(&s.manager),
        Some(json!({"reason": "Khách bận việc"})),
    )
    .await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
    assert_eq!(cancelled.body["status"], "cancelled");
    assert_eq!(cancelled.body["status_reason"], "Khách bận việc");
    assert_eq!(cancelled.body["payment_state"], "none");
    let rebooked = s.book_ha(&s.manager, &at(1, "10:00"), Some(&s.lan)).await;
    assert_eq!(rebooked.status, StatusCode::CREATED, "{}", rebooked.body);
    // Arriving tomorrow's booking today is refused.
    let arrive = call(
        &s.h.app,
        "POST",
        &format!("/appointments/{}/arrive", id_of(&rebooked)),
        Some(&s.manager),
        Some(json!({"staff_id": s.lan})),
    )
    .await;
    assert_eq!(
        code(&arrive),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("appointment.not_today")
        )
    );

    // Audit: who, what, why.
    let reasons: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, reason FROM app.audit_log WHERE entity_type = 'appointment' ORDER BY at",
    )
    .fetch_all(&s.h.owner)
    .await?;
    assert!(reasons.contains(&("appointment.cancel".into(), Some("Khách bận việc".into()))));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn past_days_are_locked_for_managers_and_open_to_the_owner(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // AC9.8: the manager cannot book into yesterday; the owner can (to enter it late).
    let manager = s.book_ha(&s.manager, &at(-1, "10:00"), Some(&s.lan)).await;
    assert_eq!(code(&manager), (StatusCode::FORBIDDEN, Some("day.locked")));
    let owner = s.book_ha(&s.owner, &at(-1, "10:00"), Some(&s.lan)).await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    let id = id_of(&owner);
    for (path, body) in [
        (format!("/appointments/{id}/cancel"), json!({"reason": "x"})),
        (
            format!("/appointments/{id}/no-show"),
            json!({"reason": "x"}),
        ),
    ] {
        let r = call(&s.h.app, "POST", &path, Some(&s.manager), Some(body)).await;
        assert_eq!(
            code(&r),
            (StatusCode::FORBIDDEN, Some("day.locked")),
            "{path}"
        );
    }
    let complete = json!({"lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.lan}],
                          "payments": [{"method": "cash", "amount": 349_000}]});
    let r = s
        .post(
            &s.manager,
            &format!("/appointments/{id}/complete"),
            complete.clone(),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some("day.locked")));
    // The owner completes it: the visit lands on yesterday, at the booked time.
    let done = s
        .post(&s.owner, &format!("/appointments/{id}/complete"), complete)
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body);
    let yesterday = (spa_today() - Duration::days(1)).to_string();
    assert_eq!(done.body["visit"]["business_date"], json!(yesterday));
    assert_eq!(done.body["visit"]["check_in_at"], owner.body["starts_at"]);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_owner_back_dates_visits_onto_the_day_of_the_service(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = |when: &str| {
        json!({"customer_id": s.duc, "check_in_at": when,
               "lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.lan}],
               "payments": [{"method": "cash", "amount": 399_000}]})
    };
    // AC5.1: today, at the time the service happened.
    let today = s.post(&s.manager, "/visits", body(&at(0, "00:05"))).await;
    assert_eq!(today.status, StatusCode::CREATED, "{}", today.body);
    assert_ne!(today.body["check_in_at"], today.body["created_at"]);

    // AC5.2: yesterday is the owner's.
    let yesterday_at = at(-1, "18:30");
    let manager = s.post(&s.manager, "/visits", body(&yesterday_at)).await;
    assert_eq!(
        code(&manager),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_not_today")
        )
    );
    let owner = s.post(&s.owner, "/visits", body(&yesterday_at)).await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);
    let yesterday = (spa_today() - Duration::days(1)).to_string();
    assert_eq!(owner.body["business_date"], json!(yesterday));
    let sale_day: (NaiveDate, NaiveDate) = sqlx::query_as(
        "SELECT s.business_date, p.business_date FROM app.sale s JOIN app.payment p ON p.sale_id = s.id WHERE s.id = $1",
    )
    .bind(Uuid::parse_str(owner.body["sale_id"].as_str().unwrap_or_default())?)
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(sale_day.0.to_string(), yesterday);
    assert_eq!(sale_day.1.to_string(), yesterday);
    let report = call(
        &s.h.app,
        "GET",
        &format!("/reports/daily?date={yesterday}"),
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(report.body["counts"]["visits"], 1, "{}", report.body);
    let audit: Value = sqlx::query_scalar(
        "SELECT after FROM app.audit_log WHERE action = 'visit.create' AND entity_id = $1",
    )
    .bind(Uuid::parse_str(&id_of(&owner))?)
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(audit["backdated"], true);
    assert_eq!(audit["recorded_on"], json!(spa_today().to_string()));

    let too_old = s.post(&s.owner, "/visits", body(&at(-400, "10:00"))).await;
    assert_eq!(
        code(&too_old),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("visit.check_in_too_old")
        )
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn visit_discounts_follow_the_package_sale_rules(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let body = |discount: Value, paid: i64| {
        json!({"customer_id": s.duc,
               "lines": [{"service_id": s.dai_thong_lac, "source": "one_off", "staff_id": s.lan},
                         {"service_id": s.shampoo, "source": "one_off", "staff_id": s.lan}],
               "discount": discount,
               "payments": [{"method": "cash", "amount": paid}]})
    };
    let listed = call(&s.h.app, "GET", "/services", Some(&s.manager), None).await;
    let shampoo_price = listed
        .body
        .as_array()
        .and_then(|l| l.iter().find(|x| x["id"] == json!(s.shampoo)))
        .and_then(|x| x["prices"]["all"].as_i64().or(x["prices"]["male"].as_i64()))
        .unwrap_or_default();
    let subtotal = 399_000 + shampoo_price;

    // AC8.1: the manager's 10% is computed by the server and reaches lines, sale and payment.
    let ten = subtotal / 10;
    let ok = s
        .post(
            &s.manager,
            "/visits",
            body(
                json!({"percent": 10, "reason": "Khách quen"}),
                subtotal - ten,
            ),
        )
        .await;
    assert_eq!(ok.status, StatusCode::CREATED, "{}", ok.body);
    assert_eq!(ok.body["subtotal"], subtotal);
    assert_eq!(ok.body["discount_amount"], ten);
    assert_eq!(ok.body["discount_reason"], "Khách quen");
    assert_eq!(ok.body["total"], subtotal - ten);
    let shares: i64 = ok.body["lines"]
        .as_array()
        .map(|l| l.iter().filter_map(|x| x["discount_amount"].as_i64()).sum())
        .unwrap_or_default();
    assert_eq!(shares, ten);
    let sale = call(
        &s.h.app,
        "GET",
        &format!("/sales/{}", ok.body["sale_id"].as_str().unwrap_or_default()),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(
        (
            sale.body["discount_amount"].as_i64(),
            sale.body["total"].as_i64()
        ),
        (Some(ten), Some(subtotal - ten))
    );

    // Paying the listed price after a discount is a mismatch: the server's total wins.
    let mismatch = s
        .post(
            &s.manager,
            "/visits",
            body(json!({"amount": 10_000, "reason": "x"}), subtotal),
        )
        .await;
    assert_eq!(
        code(&mismatch),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("payment.amount_mismatch")
        )
    );

    let over = subtotal * 11 / 100;
    let capped = s
        .post(
            &s.manager,
            "/visits",
            body(json!({"amount": over, "reason": "x"}), subtotal - over),
        )
        .await;
    assert_eq!(
        code(&capped),
        (StatusCode::FORBIDDEN, Some("discount.limit_exceeded"))
    );
    let half = subtotal / 2;
    let owner = s
        .post(
            &s.owner,
            "/visits",
            body(
                json!({"percent": 50, "reason": "Chủ spa tặng"}),
                subtotal - half,
            ),
        )
        .await;
    assert_eq!(owner.status, StatusCode::CREATED, "{}", owner.body);

    // AC8.2
    for bad in [
        json!({"percent": 10}),
        json!({"percent": 10, "amount": 1000, "reason": "x"}),
        json!({"percent": 0, "reason": "x"}),
        json!({"reason": "x"}),
    ] {
        let r = s
            .post(&s.manager, "/visits", body(bad.clone(), subtotal))
            .await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}: {}",
            r.body
        );
    }
    // Nothing to discount on a package-only visit.
    let package_only = s
        .post(
            &s.manager,
            "/visits",
            json!({"customer_id": s.ha, "discount": {"amount": 1000, "reason": "x"},
                   "lines": [{"service_id": s.shampoo, "source": "package", "customer_package_item_id": s.ha_item, "staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(package_only.status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}
