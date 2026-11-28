//! "Vòng quay may mắn" (spec product/specs/lucky-draw.md, contract architecture/lucky-draw.md):
//! the owner's draws, participants and prizes, spins drawn on the server under the draw's lock,
//! forfeits, the derived state, deletes, the audit trail, and every refusal of US1–US6.

mod common;

use axum::http::StatusCode;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call_with_headers, harness, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::idempotency::hash_route;
use kernel::testing::app_pool;
use kernel::{Actor, AppError, Permission, TenantId, UserId};
use lucky_draw::{RandomSource, RandomUnavailable};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

/// Every request from its own client address: the per-IP rate limit (burst 60) never masks a
/// result in these long flows.
fn next_ip() -> String {
    static N: AtomicU32 = AtomicU32::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("10.77.{}.{}", (n >> 8) & 0xff, n & 0xff)
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

struct Spa {
    h: Harness,
    owner: String,
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    Ok(Spa { h, owner })
}

impl Spa {
    async fn send_as(
        &self,
        session: Option<&str>,
        method: &str,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> Reply {
        let ip = next_ip();
        let mut all = vec![("X-Forwarded-For", ip.as_str())];
        all.extend_from_slice(headers);
        call_with_headers(&self.h.app, method, path, session, body, &all).await
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        self.send_as(Some(&self.owner), method, path, body, &[])
            .await
    }

    async fn put(&self, path: &str, version: &Value, body: Value) -> Reply {
        let tag = format!("\"{version}\"");
        self.send_as(
            Some(&self.owner),
            "PUT",
            path,
            Some(body),
            &[("If-Match", &tag)],
        )
        .await
    }

    /// A new draw; its detail.
    async fn draw(&self, name: &str) -> Value {
        let created = self
            .call("POST", "/lucky-draws", Some(json!({ "name": name })))
            .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        created.body
    }

    async fn add(&self, draw: &str, people: &[(&str, &str)]) -> Reply {
        let people: Vec<Value> = people
            .iter()
            .map(|(name, gender)| json!({"name": name, "gender": gender}))
            .collect();
        self.call(
            "POST",
            &format!("/lucky-draws/{draw}/participants"),
            Some(json!({ "participants": people })),
        )
        .await
    }

    async fn prize(&self, draw: &str, name: &str, eligible: &str, quantity: i64) -> Reply {
        self.call(
            "POST",
            &format!("/lucky-draws/{draw}/prizes"),
            Some(json!({"name": name, "eligible": eligible, "quantity": quantity})),
        )
        .await
    }

    async fn spin_with(&self, draw: &str, prize: &str, key: &str) -> Reply {
        self.send_as(
            Some(&self.owner),
            "POST",
            &format!("/lucky-draws/{draw}/spins"),
            Some(json!({ "prize_id": prize })),
            &[("Idempotency-Key", key)],
        )
        .await
    }

    async fn spin(&self, draw: &str, prize: &str) -> Reply {
        self.spin_with(draw, prize, &Uuid::now_v7().to_string())
            .await
    }

    async fn get(&self, draw: &str) -> Reply {
        self.call("GET", &format!("/lucky-draws/{draw}"), None)
            .await
    }

    async fn count(&self, sql: &str, draw: &str) -> Setup<i64> {
        Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(Uuid::parse_str(draw)?)
            .fetch_one(&self.h.owner)
            .await?)
    }

    /// The audit rows of a draw, oldest first: (action, before, after).
    async fn trail(&self, draw: &str) -> Setup<Vec<(String, Option<Value>, Option<Value>)>> {
        Ok(sqlx::query_as(
            "SELECT action, before, after FROM app.audit_log
             WHERE entity_type = 'lucky_draw' AND entity_id = $1 ORDER BY at, id",
        )
        .bind(Uuid::parse_str(draw)?)
        .fetch_all(&self.h.owner)
        .await?)
    }
}

/// A draw with people and prizes, entered by the owner: (draw id, detail after the last write).
async fn draw_with(
    s: &Spa,
    people: &[(&str, &str)],
    prizes: &[(&str, &str, i64)],
) -> Setup<(String, Value)> {
    let draw = text(&s.draw("Tri ân 20/11").await["id"]);
    let mut last = s.add(&draw, people).await;
    assert_eq!(last.status, StatusCode::CREATED, "{}", last.body);
    for (name, eligible, quantity) in prizes {
        last = s.prize(&draw, name, eligible, *quantity).await;
        assert_eq!(last.status, StatusCode::CREATED, "{}", last.body);
    }
    Ok((draw, last.body))
}

fn prize_id(detail: &Value, name: &str) -> String {
    detail["prizes"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["name"] == name))
        .map(|p| text(&p["id"]))
        .unwrap_or_default()
}

fn prize_of(detail: &Value, name: &str) -> Value {
    detail["prizes"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["name"] == name).cloned())
        .unwrap_or(Value::Null)
}

fn person(detail: &Value, id: &str) -> Value {
    detail["participants"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["id"] == id).cloned())
        .unwrap_or(Value::Null)
}

fn person_named(detail: &Value, name: &str) -> Value {
    detail["participants"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["name"] == name).cloned())
        .unwrap_or(Value::Null)
}

fn errors(reply: &Reply) -> Value {
    reply.body["errors"].clone()
}

fn code(reply: &Reply) -> (StatusCode, String) {
    (reply.status, text(&reply.body["code"]))
}

const SIX: &[(&str, &str)] = &[
    ("Nguyễn Thị Hoa", "female"),
    ("Trần Văn Minh", "male"),
    ("Lê Thị Mai", "female"),
    ("Phạm Văn Hùng", "male"),
    ("Đỗ Thị Lan", "female"),
    ("Võ Văn Nam", "male"),
];

// ---------------------------------------------------------------- US1: draws

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_draw_is_created_renamed_and_listed_newest_first_with_its_state(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let first = s.draw("  Tri ân\t20/11  ").await;
    assert_eq!(
        first["name"], "Tri ân 20/11",
        "trimmed, inner space collapsed"
    );
    assert_eq!(
        (
            first["status"].clone(),
            first["finished"].clone(),
            first["version"].clone()
        ),
        (json!("not_started"), json!(false), json!(1))
    );
    assert_eq!(
        (
            first["participants"].clone(),
            first["prizes"].clone(),
            first["results"].clone()
        ),
        (json!([]), json!([]), json!([]))
    );
    let first_id = text(&first["id"]);
    let (second_id, detail) = draw_with(
        &s,
        &SIX[..2],
        &[("Quà A", "male", 1), ("Quà B", "female", 2)],
    )
    .await?;
    let spun = s.spin(&second_id, &prize_id(&detail, "Quà A")).await;
    assert_eq!(spun.status, StatusCode::CREATED, "{}", spun.body);
    let (third_id, detail) = draw_with(&s, &SIX[..1], &[("Quà C", "all", 1)]).await?;
    assert_eq!(
        s.spin(&third_id, &prize_id(&detail, "Quà C")).await.status,
        StatusCode::CREATED
    );

    let list = s.call("GET", "/lucky-draws", None).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    let items = list.body["items"].as_array().cloned().unwrap_or_default();
    let ids: Vec<String> = items.iter().map(|i| text(&i["id"])).collect();
    assert_eq!(ids, [third_id.clone(), second_id.clone(), first_id.clone()]);
    let line = |i: usize| {
        let v = &items[i];
        (
            v["participant_count"].clone(),
            v["prize_slots"].clone(),
            v["awarded"].clone(),
            v["status"].clone(),
        )
    };
    assert_eq!(line(0), (json!(1), json!(1), json!(1), json!("finished")));
    assert_eq!(
        line(1),
        (json!(2), json!(3), json!(1), json!("in_progress")),
        "a woman is left for the women's prize"
    );
    assert_eq!(
        line(2),
        (json!(0), json!(0), json!(0), json!("not_started"))
    );

    // Rename: If-Match is the draw's version; a stale one is 412, none 400.
    let path = format!("/lucky-draws/{first_id}");
    let renamed = s.put(&path, &json!(1), json!({"name": "Tri ân 8/3"})).await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(
        (
            renamed.body["name"].clone(),
            renamed.body["version"].clone()
        ),
        (json!("Tri ân 8/3"), json!(2))
    );
    assert_eq!(
        renamed.headers.get("etag").and_then(|v| v.to_str().ok()),
        Some("\"2\"")
    );
    let stale = s.put(&path, &json!(1), json!({"name": "Muộn"})).await;
    assert_eq!(
        code(&stale),
        (
            StatusCode::PRECONDITION_FAILED,
            "conflict.stale_version".into()
        )
    );
    let bare = s.call("PUT", &path, Some(json!({"name": "Muộn"}))).await;
    assert_eq!(
        code(&bare),
        (
            StatusCode::BAD_REQUEST,
            "precondition.if_match_required".into()
        )
    );
    let read = s.get(&first_id).await;
    assert_eq!(
        (
            read.body["name"].clone(),
            read.headers.get("etag").and_then(|v| v.to_str().ok())
        ),
        (json!("Tri ân 8/3"), Some("\"2\""))
    );
    assert_eq!(
        s.trail(&first_id).await?,
        [
            (
                "lucky_draw.create".to_owned(),
                None,
                Some(json!({"name": "Tri ân 20/11"}))
            ),
            (
                "lucky_draw.rename".to_owned(),
                Some(json!({"name": "Tri ân 20/11"})),
                Some(json!({"name": "Tri ân 8/3"}))
            ),
        ]
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_draw_name_is_required_and_at_most_80_characters(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    for (name, expected) in [("", "required"), ("   ", "required")] {
        let refused = s
            .call("POST", "/lucky-draws", Some(json!({ "name": name })))
            .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            errors(&refused),
            json!([{"field": "name", "code": expected}])
        );
    }
    let long = "Đ".repeat(81);
    let refused = s
        .call("POST", "/lucky-draws", Some(json!({ "name": long })))
        .await;
    assert_eq!(
        errors(&refused),
        json!([{"field": "name", "code": "too_long"}])
    );
    let longest = s.draw(&"Đ".repeat(80)).await;
    assert_eq!(
        longest["name"].as_str().map(|n| n.chars().count()),
        Some(80)
    );
    let bad = s
        .call(
            "POST",
            "/lucky-draws",
            Some(json!({"name": "X", "tenant_id": "x"})),
        )
        .await;
    assert_eq!(
        errors(&bad),
        json!([{"field": "body", "code": "invalid"}]),
        "unknown fields refused"
    );
    let draw = text(&longest["id"]);
    let rename = s
        .put(
            &format!("/lucky-draws/{draw}"),
            &json!(1),
            json!({"name": ""}),
        )
        .await;
    assert_eq!(
        errors(&rename),
        json!([{"field": "name", "code": "required"}])
    );
    let drafts: i64 = sqlx::query_scalar("SELECT count(*) FROM app.lucky_draw")
        .fetch_one(&s.h.owner)
        .await?;
    assert_eq!(drafts, 1, "nothing written by a refused create");
    Ok(())
}

/// Spec L7: deleting a draw takes its people, prizes and results; the trail keeps its name and
/// counts, never a person's name (AC1.4).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn deleting_a_draw_removes_everything_and_the_trail_keeps_only_its_name_and_counts(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, SIX, &[("Quà A", "all", 2), ("Quà B", "male", 1)]).await?;
    let a = prize_id(&detail, "Quà A");
    let won = s.spin(&draw, &a).await;
    assert_eq!(won.status, StatusCode::CREATED);
    let forfeit = s
        .call(
            "POST",
            &format!(
                "/lucky-draws/{draw}/results/{}/forfeit",
                text(&won.body["result"]["id"])
            ),
            None,
        )
        .await;
    assert_eq!(forfeit.status, StatusCode::OK);
    assert_eq!(s.spin(&draw, &a).await.status, StatusCode::CREATED);
    let keep = s.draw("Chương trình khác").await;

    let deleted = s
        .call("DELETE", &format!("/lucky-draws/{draw}"), None)
        .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
    assert_eq!(s.get(&draw).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        s.call("DELETE", &format!("/lucky-draws/{draw}"), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    for table in [
        "lucky_draw",
        "lucky_draw_participant",
        "lucky_draw_prize",
        "lucky_draw_result",
    ] {
        let column = if table == "lucky_draw" {
            "id"
        } else {
            "draw_id"
        };
        let left = s
            .count(
                &format!("SELECT count(*) FROM app.{table} WHERE {column} = $1"),
                &draw,
            )
            .await?;
        assert_eq!(left, 0, "{table}");
    }
    assert_eq!(
        s.get(&text(&keep["id"])).await.status,
        StatusCode::OK,
        "other draws stay"
    );
    let trail = s.trail(&draw).await?;
    let (action, before, after) = trail.last().cloned().unwrap_or_default();
    assert_eq!(action, "lucky_draw.delete");
    assert_eq!(after, None);
    assert_eq!(
        before,
        Some(json!({"name": "Tri ân 20/11", "participants": 6, "prizes": 2, "results": 2}))
    );
    let everything = serde_json::to_string(&trail)?;
    for (name, _) in SIX {
        let in_add = trail
            .iter()
            .filter(|(a, ..)| a == "lucky_draw.participants_add")
            .any(|row| {
                serde_json::to_string(row)
                    .unwrap_or_default()
                    .contains(name)
            });
        assert!(!in_add, "the add names nobody: {name}");
    }
    assert!(everything.contains("lucky_draw.forfeit"));
    Ok(())
}

// ---------------------------------------------------------------- US2: participants

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn people_are_added_in_order_and_a_refused_batch_adds_nobody(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let draw = text(&s.draw("Tri ân").await["id"]);
    let added = s.add(&draw, &SIX[..3]).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    let names: Vec<String> = added.body["participants"]
        .as_array()
        .map(|l| l.iter().map(|p| text(&p["name"])).collect())
        .unwrap_or_default();
    assert_eq!(names, ["Nguyễn Thị Hoa", "Trần Văn Minh", "Lê Thị Mai"]);
    assert_eq!(added.body["participants"][1]["gender"], "male");
    assert_eq!(added.body["participants"][2]["position"], 3);
    assert_eq!(added.body["participants"][0]["result_id"], Value::Null);
    assert_eq!(added.body["version"], 2);
    let more = s.add(&draw, &[("Nguyễn Thị Hoa", "female")]).await;
    assert_eq!(
        more.status,
        StatusCode::CREATED,
        "a same name is only a warning on the web"
    );
    assert_eq!(more.body["participants"][3]["position"], 4);

    // Every field error of the batch, by its index; nobody of it is added.
    let refused = s
        .call(
            "POST",
            &format!("/lucky-draws/{draw}/participants"),
            Some(json!({"participants": [
                {"name": "Hợp lệ", "gender": "female"},
                {"name": " ", "gender": "female"},
                {"name": "A".repeat(121), "gender": "nu"},
                {"name": "Không rõ", "gender": "unknown"},
            ]})),
        )
        .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        errors(&refused),
        json!([
            {"field": "participants[1].name", "code": "required"},
            {"field": "participants[2].name", "code": "too_long"},
            {"field": "participants[2].gender", "code": "invalid"},
            {"field": "participants[3].gender", "code": "invalid"},
        ])
    );
    for batch in [
        json!([]),
        json!(vec![json!({"name": "X", "gender": "male"}); 501]),
    ] {
        let refused = s
            .call(
                "POST",
                &format!("/lucky-draws/{draw}/participants"),
                Some(json!({ "participants": batch })),
            )
            .await;
        assert_eq!(
            errors(&refused),
            json!([{"field": "participants", "code": "count_out_of_range"}])
        );
    }
    let missing = s
        .call(
            "POST",
            &format!("/lucky-draws/{draw}/participants"),
            Some(json!({"participants": [{"name": "Không giới tính"}]})),
        )
        .await;
    assert_eq!(
        errors(&missing),
        json!([{"field": "body", "code": "invalid"}])
    );
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_participant WHERE draw_id = $1",
            &draw
        )
        .await?,
        4
    );
    assert_eq!(
        s.trail(&draw).await?.last().map(|t| t.2.clone()),
        Some(Some(json!({"added": 1, "total": 4})))
    );
    Ok(())
}

/// 500 long names (the most one request takes) pass the route's own 256 KB cap; more is 413, and
/// every other route keeps the default 64 KB. At most 2.000 people in a draw.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_full_batch_fits_and_a_draw_holds_at_most_2000_people(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let draw = text(&s.draw("Tri ân").await["id"]);
    let path = format!("/lucky-draws/{draw}/participants");
    // 120 characters of 3 bytes each.
    let long = "Ấ".repeat(120);
    let batch =
        |n: usize| json!({"participants": vec![json!({"name": long, "gender": "female"}); n]});
    let body = batch(500);
    let size = body.to_string().len();
    assert!(size > 64 * 1024 && size < 256 * 1024, "{size} bytes");
    for _ in 0..4 {
        let added = s.call("POST", &path, Some(batch(500))).await;
        assert_eq!(added.status, StatusCode::CREATED, "{}", added.body["code"]);
    }
    let over = s
        .call(
            "POST",
            &path,
            Some(json!({"participants": [{"name": "Thêm", "gender": "male"}]})),
        )
        .await;
    assert_eq!(
        errors(&over),
        json!([{"field": "participants", "code": "too_many"}])
    );
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_participant WHERE draw_id = $1",
            &draw
        )
        .await?,
        2000
    );
    // Over the route's cap.
    let huge =
        json!({"participants": vec![json!({"name": "Ấ".repeat(200), "gender": "male"}); 500]});
    assert!(huge.to_string().len() > 256 * 1024);
    let too_big = s.call("POST", &path, Some(huge)).await;
    assert_eq!(
        code(&too_big),
        (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large".into())
    );
    // Another route of the draw keeps the default cap.
    let description = "Ấ".repeat(30_000);
    let big_prize = s
        .call(
            "POST",
            &format!("/lucky-draws/{draw}/prizes"),
            Some(json!({"name": "Quà", "description": description, "eligible": "all", "quantity": 1})),
        )
        .await;
    assert_eq!(big_prize.status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_person_without_a_result_is_edited_or_deleted_and_a_winner_never_is(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, &SIX[..2], &[("Quà", "all", 2)]).await?;
    let hoa = person_named(&detail, "Nguyễn Thị Hoa");
    let path = |p: &Value| format!("/lucky-draws/{draw}/participants/{}", text(&p["id"]));
    let edited = s
        .put(
            &path(&hoa),
            &hoa["version"],
            json!({"name": "Nguyễn Thị Hòa", "gender": "male"}),
        )
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    let now = person(&edited.body, &text(&hoa["id"]));
    assert_eq!(
        (
            now["name"].clone(),
            now["gender"].clone(),
            now["version"].clone()
        ),
        (json!("Nguyễn Thị Hòa"), json!("male"), json!(2))
    );
    let stale = s
        .put(
            &path(&hoa),
            &json!(1),
            json!({"name": "Muộn", "gender": "female"}),
        )
        .await;
    assert_eq!(
        code(&stale),
        (
            StatusCode::PRECONDITION_FAILED,
            "conflict.stale_version".into()
        )
    );
    let invalid = s
        .put(&path(&hoa), &json!(2), json!({"name": "", "gender": "x"}))
        .await;
    assert_eq!(
        errors(&invalid),
        json!([{"field": "name", "code": "required"}, {"field": "gender", "code": "invalid"}])
    );

    // Spin both, forfeit one: neither may be edited or deleted any more.
    let prize = prize_id(&detail, "Quà");
    let first = s.spin(&draw, &prize).await;
    let second = s.spin(&draw, &prize).await;
    assert_eq!(
        (first.status, second.status),
        (StatusCode::CREATED, StatusCode::CREATED)
    );
    let forfeited = s
        .call(
            "POST",
            &format!(
                "/lucky-draws/{draw}/results/{}/forfeit",
                text(&first.body["result"]["id"])
            ),
            None,
        )
        .await;
    assert_eq!(forfeited.status, StatusCode::OK);
    for result in [&first, &second] {
        let winner = person(
            &forfeited.body,
            &text(&result.body["result"]["participant_id"]),
        );
        assert_eq!(winner["result_id"], result.body["result"]["id"]);
        let edit = s
            .put(
                &path(&winner),
                &winner["version"],
                json!({"name": "Đổi", "gender": "female"}),
            )
            .await;
        assert_eq!(
            code(&edit),
            (StatusCode::CONFLICT, "lucky_draw.participant_drawn".into())
        );
        let delete = s.call("DELETE", &path(&winner), None).await;
        assert_eq!(
            code(&delete),
            (StatusCode::CONFLICT, "lucky_draw.participant_drawn".into())
        );
    }

    let late = s.add(&draw, &[("Người đến muộn", "female")]).await;
    let late = person_named(&late.body, "Người đến muộn");
    let removed = s.call("DELETE", &path(&late), None).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.body);
    assert_eq!(person(&removed.body, &text(&late["id"])), Value::Null);
    assert_eq!(
        s.call("DELETE", &path(&late), None).await.status,
        StatusCode::NOT_FOUND
    );
    let trail = s.trail(&draw).await?;
    let edit = trail
        .iter()
        .find(|(a, ..)| a == "lucky_draw.participant_update")
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (edit.1, edit.2),
        (
            Some(
                json!({"participant_id": hoa["id"], "name": "Nguyễn Thị Hoa", "gender": "female"})
            ),
            Some(json!({"participant_id": hoa["id"], "name": "Nguyễn Thị Hòa", "gender": "male"}))
        )
    );
    let delete = trail.last().cloned().unwrap_or_default();
    assert_eq!(
        (delete.0.as_str(), delete.1, delete.2),
        (
            "lucky_draw.participant_delete",
            Some(
                json!({"participant_id": late["id"], "name": "Người đến muộn", "gender": "female"})
            ),
            None
        )
    );
    Ok(())
}

// ---------------------------------------------------------------- US3: prizes

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn prizes_are_validated_and_at_most_50(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let draw = text(&s.draw("Tri ân").await["id"]);
    let path = format!("/lucky-draws/{draw}/prizes");
    let refused = s
        .call(
            "POST",
            &path,
            Some(json!({"name": "", "description": "Đ".repeat(251), "eligible": "both", "quantity": 0})),
        )
        .await;
    assert_eq!(
        errors(&refused),
        json!([
            {"field": "name", "code": "required"},
            {"field": "description", "code": "too_long"},
            {"field": "eligible", "code": "invalid"},
            {"field": "quantity", "code": "out_of_range"},
        ])
    );
    for quantity in [json!(1001), json!(-1), json!(5_000_000_000_i64)] {
        let refused = s
            .call(
                "POST",
                &path,
                Some(json!({"name": "Q", "eligible": "all", "quantity": quantity})),
            )
            .await;
        assert_eq!(
            errors(&refused),
            json!([{"field": "quantity", "code": "out_of_range"}])
        );
    }
    let long = s
        .call(
            "POST",
            &path,
            Some(json!({"name": "Q".repeat(141), "eligible": "all", "quantity": 1})),
        )
        .await;
    assert_eq!(
        errors(&long),
        json!([{"field": "name", "code": "too_long"}])
    );
    let made = s
        .call(
            "POST",
            &path,
            Some(json!({"name": "Cổ vai gáy", "description": "  ", "eligible": "female", "quantity": 1000})),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let prize = prize_of(&made.body, "Cổ vai gáy");
    assert_eq!(
        (
            prize["description"].clone(),
            prize["eligible"].clone(),
            prize["quantity"].clone(),
            prize["remaining"].clone(),
            prize["awarded"].clone(),
            prize["eligible_count"].clone(),
            prize["position"].clone(),
        ),
        (
            json!(null),
            json!("female"),
            json!(1000),
            json!(1000),
            json!(0),
            json!(0),
            json!(1)
        ),
        "a blank description is none"
    );
    for i in 2..=50 {
        let r = s.prize(&draw, &format!("Quà {i}"), "all", 1).await;
        assert_eq!(r.status, StatusCode::CREATED, "{i}");
    }
    let fifty_first = s.prize(&draw, "Quà 51", "all", 1).await;
    assert_eq!(
        errors(&fifty_first),
        json!([{"field": "prizes", "code": "too_many"}])
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_spun_prize_keeps_its_target_its_awarded_slots_and_its_row(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // A goes to the men only, so B's women are all still there whatever A draws.
    let (draw, detail) = draw_with(&s, SIX, &[("Quà A", "male", 3), ("Quà B", "male", 1)]).await?;
    let a = prize_of(&detail, "Quà A");
    let b = prize_of(&detail, "Quà B");
    let path = |p: &Value| format!("/lucky-draws/{draw}/prizes/{}", text(&p["id"]));
    // Before any result: everything may change, the prize may go.
    let retargeted = s
        .put(
            &path(&b),
            &b["version"],
            json!({"name": "Quà B (nữ)", "description": "Liệu trình 60 phút", "eligible": "female", "quantity": 2}),
        )
        .await;
    assert_eq!(retargeted.status, StatusCode::OK, "{}", retargeted.body);
    let b = prize_of(&retargeted.body, "Quà B (nữ)");
    assert_eq!(
        (
            b["eligible"].clone(),
            b["eligible_count"].clone(),
            b["version"].clone(),
            b["description"].clone()
        ),
        (
            json!("female"),
            json!(3),
            json!(2),
            json!("Liệu trình 60 phút")
        )
    );
    // Two results on A, one forfeited.
    let first = s.spin(&draw, &text(&a["id"])).await;
    assert_eq!(
        s.spin(&draw, &text(&a["id"])).await.status,
        StatusCode::CREATED
    );
    let forfeit = s
        .call(
            "POST",
            &format!(
                "/lucky-draws/{draw}/results/{}/forfeit",
                text(&first.body["result"]["id"])
            ),
            None,
        )
        .await;
    let a = prize_of(&forfeit.body, "Quà A");
    assert_eq!(
        (a["awarded"].clone(), a["remaining"].clone()),
        (json!(1), json!(2))
    );
    let body = |eligible: &str, quantity: i64| json!({"name": "Quà A mới", "description": null, "eligible": eligible, "quantity": quantity});
    let retarget = s.put(&path(&a), &a["version"], body("female", 3)).await;
    assert_eq!(
        code(&retarget),
        (StatusCode::CONFLICT, "lucky_draw.prize_drawn".into())
    );
    let below = s.put(&path(&a), &a["version"], body("male", 0)).await;
    assert_eq!(
        errors(&below),
        json!([{"field": "quantity", "code": "out_of_range"}])
    );
    // One kept result: the quantity may go down to 1, not below.
    let kept = s.put(&path(&a), &a["version"], body("male", 1)).await;
    assert_eq!(kept.status, StatusCode::OK, "{}", kept.body);
    let a = prize_of(&kept.body, "Quà A mới");
    assert_eq!(
        (a["quantity"].clone(), a["remaining"].clone()),
        (json!(1), json!(0))
    );
    let stale = s.put(&path(&a), &json!(1), body("male", 2)).await;
    assert_eq!(
        code(&stale),
        (
            StatusCode::PRECONDITION_FAILED,
            "conflict.stale_version".into()
        )
    );
    // B: one result kept → its quantity may go down to 1; a spin then finds no slot; back to 2,
    // a second result → 1 is below what it awarded.
    let second_kept = s.spin(&draw, &text(&b["id"])).await;
    assert_eq!(second_kept.status, StatusCode::CREATED);
    let b_now = prize_of(&second_kept.body["draw"], "Quà B (nữ)");
    let refused = s
        .put(
            &path(&b_now),
            &b_now["version"],
            json!({"name": "Quà B (nữ)", "eligible": "female", "quantity": 1}),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::OK,
        "1 kept result, quantity 1 is fine"
    );
    let b_now = prize_of(&refused.body, "Quà B (nữ)");
    let s2 = s.spin(&draw, &text(&b_now["id"])).await;
    assert_eq!(
        code(&s2),
        (StatusCode::CONFLICT, "lucky_draw.prize_exhausted".into())
    );
    let raise = s
        .put(
            &path(&b_now),
            &b_now["version"],
            json!({"name": "Quà B (nữ)", "eligible": "female", "quantity": 2}),
        )
        .await;
    assert_eq!(raise.status, StatusCode::OK);
    let b_now = prize_of(&raise.body, "Quà B (nữ)");
    assert_eq!(
        s.spin(&draw, &text(&b_now["id"])).await.status,
        StatusCode::CREATED
    );
    let b_now = prize_of(&s.get(&draw).await.body, "Quà B (nữ)");
    let shrink = s
        .put(
            &path(&b_now),
            &b_now["version"],
            json!({"name": "Quà B (nữ)", "eligible": "female", "quantity": 1}),
        )
        .await;
    assert_eq!(
        code(&shrink),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "lucky_draw.quantity_below_awarded".into()
        )
    );
    // A prize with a result (forfeited ones too) stays; one without goes.
    let delete = s.call("DELETE", &path(&a), None).await;
    assert_eq!(
        code(&delete),
        (StatusCode::CONFLICT, "lucky_draw.prize_drawn".into())
    );
    let spare = s.prize(&draw, "Quà thừa", "all", 1).await;
    let spare = prize_of(&spare.body, "Quà thừa");
    assert_eq!(spare["position"], 3);
    let gone = s.call("DELETE", &path(&spare), None).await;
    assert_eq!(gone.status, StatusCode::OK);
    assert_eq!(prize_of(&gone.body, "Quà thừa"), Value::Null);
    let trail = s.trail(&draw).await?;
    let (action, before, after) = trail.last().cloned().unwrap_or_default();
    assert_eq!(
        (action.as_str(), before, after),
        (
            "lucky_draw.prize_delete",
            Some(
                json!({"prize_id": spare["id"], "name": "Quà thừa", "description": null, "eligible": "all", "quantity": 1})
            ),
            None
        )
    );
    Ok(())
}

// ---------------------------------------------------------------- US4: spins

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_spin_draws_an_eligible_person_and_takes_a_slot_until_the_draw_is_finished(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(
        &s,
        SIX,
        &[
            ("Chỉ nữ", "female", 2),
            ("Cả hai", "all", 2),
            ("Chỉ nam", "male", 5),
        ],
    )
    .await?;
    let women = prize_id(&detail, "Chỉ nữ");
    let everyone = prize_id(&detail, "Cả hai");
    let men = prize_id(&detail, "Chỉ nam");
    let gender = |d: &Value, p: &str| person(d, p)["gender"].clone();
    let mut winners = BTreeSet::new();
    for seq in 1..=2 {
        let spun = s.spin(&draw, &women).await;
        assert_eq!(spun.status, StatusCode::CREATED, "{}", spun.body);
        let result = &spun.body["result"];
        assert_eq!(result["seq"], seq);
        assert_eq!(result["prize_id"], json!(women));
        let winner = text(&result["participant_id"]);
        assert_eq!(gender(&spun.body["draw"], &winner), "female");
        assert!(winners.insert(winner.clone()), "nobody wins twice");
        assert_eq!(
            person(&spun.body["draw"], &winner)["result_id"],
            result["id"]
        );
        assert_eq!(
            spun.headers.get("etag").and_then(|v| v.to_str().ok()),
            Some(format!("\"{}\"", spun.body["draw"]["version"]).as_str())
        );
        assert_eq!(spun.body["draw"]["status"], "in_progress");
    }
    let after = s.get(&draw).await.body;
    let w = prize_of(&after, "Chỉ nữ");
    assert_eq!(
        (
            w["awarded"].clone(),
            w["remaining"].clone(),
            w["eligible_count"].clone()
        ),
        (json!(2), json!(0), json!(1))
    );
    assert_eq!(prize_of(&after, "Cả hai")["eligible_count"], 4);
    let exhausted = s.spin(&draw, &women).await;
    assert_eq!(
        code(&exhausted),
        (StatusCode::CONFLICT, "lucky_draw.prize_exhausted".into())
    );
    for _ in 0..2 {
        let spun = s.spin(&draw, &everyone).await;
        assert_eq!(spun.status, StatusCode::CREATED);
        assert!(winners.insert(text(&spun.body["result"]["participant_id"])));
    }
    // Whoever is left: the men the "Cả hai" spins did not take.
    let left_men = 3 - winners
        .iter()
        .filter(|w| gender(&after, w) == "male")
        .count();
    for _ in 0..left_men {
        let spun = s.spin(&draw, &men).await;
        assert_eq!(spun.status, StatusCode::CREATED);
        assert_eq!(
            gender(
                &spun.body["draw"],
                &text(&spun.body["result"]["participant_id"])
            ),
            "male"
        );
    }
    let none_left = s.spin(&draw, &men).await;
    assert_eq!(
        code(&none_left),
        (StatusCode::CONFLICT, "lucky_draw.no_eligible".into())
    );
    let done = s.get(&draw).await.body;
    let seqs: Vec<i64> = done["results"]
        .as_array()
        .map(|l| l.iter().filter_map(|r| r["seq"].as_i64()).collect())
        .unwrap_or_default();
    let expected: Vec<i64> = (1..=i64::try_from(4 + left_men)?).collect();
    assert_eq!(seqs, expected, "spin order, no gap, no repeat");
    // The remaining woman and the men's slots: nothing can be drawn any more.
    assert_eq!(prize_of(&done, "Chỉ nam")["eligible_count"], 0);
    assert_eq!(
        (done["status"].clone(), done["finished"].clone()),
        (json!("finished"), json!(true))
    );
    let list = s.call("GET", "/lucky-draws", None).await.body;
    assert_eq!(list["items"][0]["status"], "finished");
    // Every spin's trail names the spin, the prize and the winner.
    let spins: Vec<Value> = s
        .trail(&draw)
        .await?
        .into_iter()
        .filter(|(a, ..)| a == "lucky_draw.spin")
        .filter_map(|(_, _, after)| after)
        .collect();
    assert_eq!(spins.len(), seqs.len());
    let first = &spins[0];
    assert_eq!(first["seq"], 1);
    assert_eq!(first["prize"], "Chỉ nữ");
    assert_eq!(
        first["winner"],
        person(&done, &text(&first["participant_id"]))["name"]
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_spin_is_refused_for_a_prize_of_another_draw_and_without_people_or_key(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let (draw, _) = draw_with(&s, SIX, &[("Quà", "all", 1)]).await?;
    let (_, other_detail) = draw_with(&s, &SIX[..1], &[("Quà khác", "all", 1)]).await?;
    let foreign = prize_id(&other_detail, "Quà khác");
    assert_eq!(s.spin(&draw, &foreign).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        s.spin(&draw, &Uuid::now_v7().to_string()).await.status,
        StatusCode::NOT_FOUND
    );
    let empty = text(&s.draw("Chưa có ai").await["id"]);
    let lonely = s.prize(&empty, "Quà", "all", 1).await;
    let none = s.spin(&empty, &prize_id(&lonely.body, "Quà")).await;
    assert_eq!(
        code(&none),
        (StatusCode::CONFLICT, "lucky_draw.no_eligible".into())
    );
    let no_key = s
        .call(
            "POST",
            &format!("/lucky-draws/{draw}/spins"),
            Some(json!({"prize_id": foreign})),
        )
        .await;
    assert_eq!(
        code(&no_key),
        (StatusCode::BAD_REQUEST, "idempotency.key_required".into())
    );
    let bad_key = s.spin_with(&draw, &foreign, "not-a-uuid").await;
    assert_eq!(
        code(&bad_key),
        (StatusCode::BAD_REQUEST, "idempotency.key_invalid".into())
    );
    let bad_body = s
        .send_as(
            Some(&s.owner),
            "POST",
            &format!("/lucky-draws/{draw}/spins"),
            Some(json!({"prize_id": foreign, "participant_id": foreign})),
            &[("Idempotency-Key", &Uuid::now_v7().to_string())],
        )
        .await;
    assert_eq!(
        errors(&bad_body),
        json!([{"field": "body", "code": "invalid"}]),
        "the client never names a winner"
    );
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_result WHERE draw_id = $1",
            &draw
        )
        .await?,
        0
    );
    let malformed = s.call("GET", "/lucky-draws/not-a-uuid", None).await;
    assert_eq!(
        code(&malformed),
        (StatusCode::BAD_REQUEST, "path.invalid".into())
    );
    Ok(())
}

/// AC4.4 / AC4.6: the same key sent again answers with the first spin, never a second one; the
/// key on another draw or prize is a mismatch; a refused spin leaves the key free.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_retried_spin_replays_the_first_result(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, SIX, &[("Quà", "all", 3), ("Quà nam", "male", 1)]).await?;
    let prize = prize_id(&detail, "Quà");
    let key = Uuid::now_v7().to_string();
    let first = s.spin_with(&draw, &prize, &key).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    let again = s.spin_with(&draw, &prize, &key).await;
    assert_eq!(again.status, StatusCode::CREATED, "{}", again.body);
    assert_eq!(again.body["result"], first.body["result"]);
    assert_eq!(
        again.body["draw"]["results"].as_array().map(Vec::len),
        Some(1)
    );
    let (twice_a, twice_b) = tokio::join!(
        s.spin_with(&draw, &prize, &key),
        s.spin_with(&draw, &prize, &key)
    );
    assert_eq!(twice_a.body["result"], first.body["result"]);
    assert_eq!(twice_b.body["result"], first.body["result"]);
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_result WHERE draw_id = $1",
            &draw
        )
        .await?,
        1
    );
    let other_prize = s
        .spin_with(&draw, &prize_id(&detail, "Quà nam"), &key)
        .await;
    assert_eq!(
        code(&other_prize),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "idempotency.key_reuse_mismatch".into()
        )
    );
    let (other_draw, other_detail) = draw_with(&s, &SIX[..1], &[("Quà", "all", 1)]).await?;
    let other = s
        .spin_with(&other_draw, &prize_id(&other_detail, "Quà"), &key)
        .await;
    assert_eq!(
        code(&other),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "idempotency.key_reuse_mismatch".into()
        )
    );
    // The record keeps the result's id only, never a name.
    let stored: Value =
        sqlx::query_scalar("SELECT response_body FROM app.idempotency_record WHERE key = $1::uuid")
            .bind(&key)
            .fetch_one(&s.h.owner)
            .await?;
    assert_eq!(stored, json!({"result_id": first.body["result"]["id"]}));
    // A refused spin rolls its key back: the same key works once the cause is gone.
    let men = prize_id(&detail, "Quà nam");
    let exhausting = s.spin(&draw, &men).await;
    assert_eq!(exhausting.status, StatusCode::CREATED);
    let retry_key = Uuid::now_v7().to_string();
    let refused = s.spin_with(&draw, &men, &retry_key).await;
    assert_eq!(
        code(&refused),
        (StatusCode::CONFLICT, "lucky_draw.prize_exhausted".into())
    );
    let men_prize = prize_of(&s.get(&draw).await.body, "Quà nam");
    let raised = s
        .put(
            &format!("/lucky-draws/{draw}/prizes/{men}"),
            &men_prize["version"],
            json!({"name": "Quà nam", "eligible": "male", "quantity": 2}),
        )
        .await;
    assert_eq!(raised.status, StatusCode::OK);
    assert_eq!(
        s.spin_with(&draw, &men, &retry_key).await.status,
        StatusCode::CREATED
    );
    // A replay after the draw is gone has nothing to answer with.
    assert_eq!(
        s.call("DELETE", &format!("/lucky-draws/{draw}"), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        s.spin_with(&draw, &prize, &key).await.status,
        StatusCode::NOT_FOUND
    );
    Ok(())
}

/// Waits until `n` sessions of the test database wait on a lock.
async fn until_waiting(owner: &PgPool, n: i64) -> Setup<()> {
    for _ in 0..2_000 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(owner)
        .await?;
        if waiting >= n {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Err(format!("{n} requests never waited on the draw's lock").into())
}

/// AC4.5: spins of one draw from several tabs at once. The draw's row is held by another
/// transaction while three spins of a two-slot prize start; all three wait on it, then run one
/// after the other: two winners, two different people, seq 1 and 2, the third refused.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn concurrent_spins_take_turns_and_never_over_award(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, &SIX[..3], &[("Quà", "all", 2)]).await?;
    let prize = prize_id(&detail, "Quà");
    let mut holder = s.h.owner.begin().await?;
    sqlx::query("SELECT 1 FROM app.lucky_draw WHERE id = $1::uuid FOR UPDATE")
        .bind(&draw)
        .execute(&mut *holder)
        .await?;
    let spins: Vec<_> = (0..3)
        .map(|_| {
            let (app, token, path, body) = (
                s.h.app.clone(),
                s.owner.clone(),
                format!("/lucky-draws/{draw}/spins"),
                json!({ "prize_id": prize }),
            );
            tokio::spawn(async move {
                let key = Uuid::now_v7().to_string();
                let ip = next_ip();
                call_with_headers(
                    &app,
                    "POST",
                    &path,
                    Some(&token),
                    Some(body),
                    &[("Idempotency-Key", &key), ("X-Forwarded-For", &ip)],
                )
                .await
            })
        })
        .collect();
    until_waiting(&s.h.owner, 3).await?;
    holder.commit().await?;
    let mut created = Vec::new();
    let mut refused = Vec::new();
    for spin in spins {
        let reply = spin.await?;
        if reply.status == StatusCode::CREATED {
            created.push(reply);
        } else {
            refused.push(code(&reply));
        }
    }
    assert_eq!(created.len(), 2);
    assert_eq!(
        refused,
        [(
            StatusCode::CONFLICT,
            "lucky_draw.prize_exhausted".to_owned()
        )]
    );
    let mut seqs: Vec<i64> = created
        .iter()
        .filter_map(|r| r.body["result"]["seq"].as_i64())
        .collect();
    seqs.sort_unstable();
    assert_eq!(seqs, [1, 2]);
    assert_ne!(
        created[0].body["result"]["participant_id"],
        created[1].body["result"]["participant_id"]
    );
    let after = s.get(&draw).await.body;
    assert_eq!(
        (
            prize_of(&after, "Quà")["awarded"].clone(),
            after["results"].as_array().map(Vec::len)
        ),
        (json!(2), Some(2))
    );
    // Two connections, no holder: whoever comes second waits for the first and sees its result.
    let (last_draw, last_detail) = draw_with(&s, &SIX[..2], &[("Quà cuối", "all", 1)]).await?;
    let last = prize_id(&last_detail, "Quà cuối");
    let (a, b) = tokio::join!(s.spin(&last_draw, &last), s.spin(&last_draw, &last));
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::CREATED, StatusCode::CONFLICT]);
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_result WHERE draw_id = $1",
            &last_draw
        )
        .await?,
        1
    );
    let broken: Vec<(String, i64)> = sqlx::query_as(
        "SELECT check_name, violations FROM app.invariant_violations()
         WHERE check_name LIKE 'lucky_draw%' AND violations > 0",
    )
    .fetch_all(&app_pool(&s.h.owner).await?)
    .await?;
    assert_eq!(broken, []);
    Ok(())
}

/// Hands out the scripted words in order, then fails.
struct Script(Vec<u64>);

impl RandomSource for Script {
    fn next_u64(&mut self) -> Result<u64, RandomUnavailable> {
        if self.0.is_empty() {
            Err(RandomUnavailable)
        } else {
            Ok(self.0.remove(0))
        }
    }
}

/// The winner is the eligible person (entry order) at the random index; a word that would bias
/// the pick is drawn again; a failing source draws nobody and writes nothing.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_winner_is_the_eligible_person_at_the_random_index(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, SIX, &[("Chỉ nữ", "female", 3)]).await?;
    let prize = Uuid::parse_str(&prize_id(&detail, "Chỉ nữ"))?;
    let draw_id = Uuid::parse_str(&draw)?;
    let (tenant, user): (Uuid, Uuid) =
        sqlx::query_as("SELECT d.tenant_id, d.created_by FROM app.lucky_draw d WHERE d.id = $1")
            .bind(draw_id)
            .fetch_one(&s.h.owner)
            .await?;
    let actor = Actor::new(
        UserId::from_uuid(user),
        TenantId::from_uuid(tenant),
        [Permission::LuckyDrawManage],
        None,
    );
    let pool = app_pool(&s.h.owner).await?;
    let spin = |words: Vec<u64>| {
        let (pool, actor) = (pool.clone(), actor.clone());
        async move {
            let key = Uuid::now_v7();
            let hash = hash_route(
                "POST",
                "/lucky-draws/{id}/spins",
                Some(draw_id),
                key.as_bytes(),
            );
            lucky_draw::service::spin(&pool, &actor, key, hash, draw_id, prize, &mut Script(words))
                .await
        }
    };
    let name = |d: &lucky_draw::service::Detail, id: Uuid| {
        d.participants
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    };
    // Women in entry order: Hoa, Mai, Lan. 4 % 3 = 1 → Mai.
    let (result, d) = spin(vec![4]).await?;
    assert_eq!(name(&d, result.participant_id), "Lê Thị Mai");
    // Left: Hoa, Lan. u64::MAX is rejected for 2 (2^64 − 1 is odd); 7 % 2 = 1 → Lan.
    let (result, d) = spin(vec![u64::MAX, 7]).await?;
    assert_eq!(name(&d, result.participant_id), "Đỗ Thị Lan");
    // A source that fails: no winner, no result, no audit entry.
    let before = s
        .count(
            "SELECT count(*) FROM app.audit_log WHERE entity_id = $1",
            &draw,
        )
        .await?;
    let failed = spin(vec![]).await;
    assert!(matches!(failed, Err(AppError::Internal(_))), "{failed:?}");
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_result WHERE draw_id = $1",
            &draw
        )
        .await?,
        2
    );
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.audit_log WHERE entity_id = $1",
            &draw
        )
        .await?,
        before
    );
    let (result, d) = spin(vec![0]).await?;
    assert_eq!(name(&d, result.participant_id), "Nguyễn Thị Hoa");
    assert_eq!(result.seq, 3);
    assert!(
        d.status.finished(),
        "every woman won, and the prize is out of slots"
    );
    // The right is checked before anything else.
    let manager = Actor::new(
        UserId::from_uuid(user),
        TenantId::from_uuid(tenant),
        [],
        None,
    );
    let key = Uuid::now_v7();
    let refused = lucky_draw::service::spin(
        &pool,
        &manager,
        key,
        [0; 32],
        draw_id,
        prize,
        &mut Script(vec![0]),
    )
    .await;
    assert!(matches!(
        refused,
        Err(AppError::Forbidden {
            code: "auth.forbidden"
        })
    ));
    Ok(())
}

// ---------------------------------------------------------------- US5: forfeits

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_forfeit_gives_the_slot_back_once_and_keeps_the_person_out(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, &SIX[..2], &[("Quà", "all", 1)]).await?;
    let prize = prize_id(&detail, "Quà");
    let won = s.spin(&draw, &prize).await;
    let result = text(&won.body["result"]["id"]);
    let winner = text(&won.body["result"]["participant_id"]);
    let path = format!("/lucky-draws/{draw}/results/{result}/forfeit");
    let forfeited = s.call("POST", &path, None).await;
    assert_eq!(forfeited.status, StatusCode::OK, "{}", forfeited.body);
    let r = &forfeited.body["results"][0];
    assert_eq!(r["id"], json!(result));
    assert!(r["forfeited_at"].is_string());
    let q = prize_of(&forfeited.body, "Quà");
    assert_eq!(
        (
            q["awarded"].clone(),
            q["remaining"].clone(),
            q["eligible_count"].clone()
        ),
        (json!(0), json!(1), json!(1)),
        "the slot is back; the absent winner is not eligible again"
    );
    assert_eq!(person(&forfeited.body, &winner)["result_id"], json!(result));
    assert_eq!(forfeited.body["status"], "in_progress");
    let twice = s.call("POST", &path, None).await;
    assert_eq!(
        code(&twice),
        (StatusCode::CONFLICT, "lucky_draw.result_forfeited".into())
    );
    let next = s.spin(&draw, &prize).await;
    assert_eq!(next.status, StatusCode::CREATED);
    assert_ne!(text(&next.body["result"]["participant_id"]), winner);
    assert_eq!(next.body["result"]["seq"], 2);
    assert_eq!(next.body["draw"]["status"], "finished");
    // A result of another draw is not found through this one.
    let (other, other_detail) = draw_with(&s, &SIX[..1], &[("Quà", "all", 1)]).await?;
    let other_result = s.spin(&other, &prize_id(&other_detail, "Quà")).await;
    let crossed = s
        .call(
            "POST",
            &format!(
                "/lucky-draws/{draw}/results/{}/forfeit",
                text(&other_result.body["result"]["id"])
            ),
            None,
        )
        .await;
    assert_eq!(crossed.status, StatusCode::NOT_FOUND);
    let trail = s.trail(&draw).await?;
    let forfeit = trail
        .iter()
        .find(|(a, ..)| a == "lucky_draw.forfeit")
        .and_then(|(_, _, after)| after.clone())
        .unwrap_or_default();
    assert_eq!(forfeit["result_id"], json!(result));
    assert_eq!(forfeit["winner"], person(&forfeited.body, &winner)["name"]);
    assert_eq!(forfeit["prize"], "Quà");
    Ok(())
}

/// The database holds a result to "forfeited once, nothing else", even for a buggy writer: the
/// runtime role cannot delete one or change another column.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_result_is_final_in_the_database_too(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let (draw, detail) = draw_with(&s, &SIX[..2], &[("Quà", "all", 2)]).await?;
    let won = s.spin(&draw, &prize_id(&detail, "Quà")).await;
    let result = Uuid::parse_str(&text(&won.body["result"]["id"]))?;
    let tenant: Uuid =
        sqlx::query_scalar("SELECT tenant_id FROM app.lucky_draw WHERE id = $1::uuid")
            .bind(&draw)
            .fetch_one(&s.h.owner)
            .await?;
    let app = app_pool(&s.h.owner).await?;
    let mut tx = kernel::TenantTx::begin(&app, TenantId::from_uuid(tenant)).await?;
    let deleted = sqlx::query("DELETE FROM app.lucky_draw_result WHERE id = $1")
        .bind(result)
        .execute(tx.conn())
        .await;
    assert!(
        deleted.is_err_and(|e| e.to_string().contains("permission denied")),
        "app_rw has no DELETE on results"
    );
    drop(tx);
    let mut tx = kernel::TenantTx::begin(&app, TenantId::from_uuid(tenant)).await?;
    let moved = sqlx::query("UPDATE app.lucky_draw_result SET seq = 9 WHERE id = $1")
        .bind(result)
        .execute(tx.conn())
        .await;
    assert!(moved.is_err(), "seq is not an updatable column");
    drop(tx);
    // The owner role: the trigger refuses anything but one forfeit, and a delete outside a
    // draw's cascade.
    let rewrite = sqlx::query("UPDATE app.lucky_draw_result SET forfeited_at = drawn_at, forfeited_by = drawn_by, prize_id = prize_id WHERE id = $1")
        .bind(result)
        .execute(&s.h.owner)
        .await;
    assert!(rewrite.is_ok(), "{rewrite:?}");
    let again = sqlx::query(
        "UPDATE app.lucky_draw_result SET forfeited_at = clock_timestamp() WHERE id = $1",
    )
    .bind(result)
    .execute(&s.h.owner)
    .await;
    assert!(again.is_err_and(|e| e.to_string().contains("forfeited, once")));
    let direct = sqlx::query("DELETE FROM app.lucky_draw_result WHERE id = $1")
        .bind(result)
        .execute(&s.h.owner)
        .await;
    assert!(direct.is_err_and(|e| e.to_string().contains("only with its draw")));
    Ok(())
}

// ---------------------------------------------------------------- who may

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_reaches_a_draw_and_only_their_spas(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    add_member(
        &s.h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &s.h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV",
        SystemRole::Technician,
    )
    .await?;
    create_tenant(&s.h.owner, "other", "Spa khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "other",
        OTHER_OWNER_PHONE,
        "Chủ spa khác",
        SystemRole::Owner,
    )
    .await?;
    let (draw, detail) = draw_with(&s, &SIX[..2], &[("Quà", "all", 1)]).await?;
    let prize = prize_id(&detail, "Quà");
    for phone in [MANAGER_PHONE, TECH_PHONE] {
        let token = session_for(&s.h, phone).await;
        for (method, path, body) in [
            ("GET", "/lucky-draws".to_owned(), None),
            ("GET", format!("/lucky-draws/{draw}"), None),
            (
                "POST",
                "/lucky-draws".to_owned(),
                Some(json!({"name": "X"})),
            ),
            // Even a malformed id: the right comes first.
            ("GET", "/lucky-draws/not-a-uuid".to_owned(), None),
        ] {
            let reply = s.send_as(Some(&token), method, &path, body, &[]).await;
            assert_eq!(
                code(&reply),
                (StatusCode::FORBIDDEN, "auth.forbidden".into()),
                "{phone} {method} {path}"
            );
        }
        let spin = s
            .send_as(
                Some(&token),
                "POST",
                &format!("/lucky-draws/{draw}/spins"),
                Some(json!({ "prize_id": prize })),
                &[("Idempotency-Key", &Uuid::now_v7().to_string())],
            )
            .await;
        assert_eq!(
            code(&spin),
            (StatusCode::FORBIDDEN, "auth.forbidden".into())
        );
    }
    let anonymous = s.send_as(None, "GET", "/lucky-draws", None, &[]).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    // Another spa's owner: the draw does not exist for them, and their list is empty.
    let intruder = session_for(&s.h, OTHER_OWNER_PHONE).await;
    let read = s
        .send_as(
            Some(&intruder),
            "GET",
            &format!("/lucky-draws/{draw}"),
            None,
            &[],
        )
        .await;
    assert_eq!(read.status, StatusCode::NOT_FOUND);
    let spin = s
        .send_as(
            Some(&intruder),
            "POST",
            &format!("/lucky-draws/{draw}/spins"),
            Some(json!({ "prize_id": prize })),
            &[("Idempotency-Key", &Uuid::now_v7().to_string())],
        )
        .await;
    assert_eq!(spin.status, StatusCode::NOT_FOUND);
    let list = s
        .send_as(Some(&intruder), "GET", "/lucky-draws", None, &[])
        .await;
    assert_eq!(list.body, json!({"items": []}));
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.lucky_draw_result WHERE draw_id = $1",
            &draw
        )
        .await?,
        0
    );
    // The permission is the owner's in every spa, and in the owner template (0055).
    let holders: Vec<String> = sqlx::query_scalar(
        "SELECT r.key FROM app.role_permission rp JOIN app.role r ON r.tenant_id = rp.tenant_id AND r.id = rp.role_id
         WHERE rp.permission = 'lucky_draw.manage' ORDER BY 1",
    )
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(holders, ["owner", "owner"]);
    let template: Vec<String> = sqlx::query_scalar(
        "SELECT role_key FROM app.role_template_permission WHERE permission = 'lucky_draw.manage'",
    )
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(template, ["owner"]);
    Ok(())
}

/// Every write of a draw is in the owner's trail and in "Nhật ký thao tác", grouped under the
/// lucky draw, named by the draw (a deleted one by the name its delete kept).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn every_write_is_audited_and_read_in_the_activity_log(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&s.h.owner)
        .await?;
    common::enable_feature(&s.h, tenant, "activity_log").await?;
    let (draw, detail) = draw_with(&s, &SIX[..3], &[("Quà", "all", 2)]).await?;
    let renamed = s
        .put(
            &format!("/lucky-draws/{draw}"),
            &detail["version"],
            json!({"name": "Tri ân Tết"}),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    let hoa = person_named(&detail, "Nguyễn Thị Hoa");
    let minh = person_named(&detail, "Trần Văn Minh");
    let mai = person_named(&detail, "Lê Thị Mai");
    let p = |x: &Value| format!("/lucky-draws/{draw}/participants/{}", text(&x["id"]));
    assert_eq!(
        s.put(
            &p(&hoa),
            &hoa["version"],
            json!({"name": "Nguyễn Thị Hòa", "gender": "female"})
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        s.call("DELETE", &p(&minh), None).await.status,
        StatusCode::OK
    );
    let prize = prize_of(&detail, "Quà");
    let q = format!("/lucky-draws/{draw}/prizes/{}", text(&prize["id"]));
    assert_eq!(
        s.put(
            &q,
            &prize["version"],
            json!({"name": "Quà lớn", "eligible": "female", "quantity": 1})
        )
        .await
        .status,
        StatusCode::OK
    );
    let spare = s.prize(&draw, "Quà phụ", "all", 1).await;
    let spare = prize_of(&spare.body, "Quà phụ");
    assert_eq!(
        s.call(
            "DELETE",
            &format!("/lucky-draws/{draw}/prizes/{}", text(&spare["id"])),
            None
        )
        .await
        .status,
        StatusCode::OK
    );
    let won = s.spin(&draw, &text(&prize["id"])).await;
    let winner = person(
        &won.body["draw"],
        &text(&won.body["result"]["participant_id"]),
    );
    assert_eq!(
        s.call(
            "POST",
            &format!(
                "/lucky-draws/{draw}/results/{}/forfeit",
                text(&won.body["result"]["id"])
            ),
            None
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        s.call("DELETE", &format!("/lucky-draws/{draw}"), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );

    let actions: BTreeSet<String> = s.trail(&draw).await?.into_iter().map(|(a, ..)| a).collect();
    let every: BTreeSet<String> = kernel::audit::Action::ALL
        .iter()
        .map(|a| a.as_str().to_owned())
        .filter(|a| a.starts_with("lucky_draw."))
        .collect();
    assert_eq!(every.len(), 11);
    assert_eq!(actions, every, "one entry per kind of write");
    let actors: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT actor_kind, u.full_name FROM app.audit_log a
         JOIN app.user_account u ON u.id = a.actor_user_id WHERE a.entity_id = $1::uuid",
    )
    .bind(&draw)
    .fetch_all(&s.h.owner)
    .await?;
    assert_eq!(actors, [("user".to_owned(), "Chủ Spa".to_owned())]);

    let page = s
        .call("GET", "/audit-log?limit=100&kind=lucky_draw", None)
        .await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let items = page.body["items"].as_array().cloned().unwrap_or_default();
    // 11 kinds of write, the prize created twice ("Quà" and "Quà phụ").
    assert_eq!(items.len(), 12, "{items:?}");
    for item in &items {
        assert_eq!(item["kind"], "lucky_draw");
        assert_eq!(
            item["subject"],
            json!({"kind": "lucky_draw", "name": "Tri ân Tết"}),
            "the deleted draw by the name its delete kept"
        );
    }
    let entry = |action: &str| {
        items
            .iter()
            .find(|i| i["action"] == action)
            .cloned()
            .unwrap_or(Value::Null)
    };
    let spin = entry("lucky_draw.spin");
    assert_eq!(spin["detail"], winner["name"]);
    assert_eq!(
        spin["changes"],
        json!([
            {"field": "seq", "format": "number", "new": "1"},
            {"field": "prize", "format": "text", "new": "Quà lớn"},
        ])
    );
    assert_eq!(entry("lucky_draw.forfeit")["detail"], winner["name"]);
    assert_eq!(
        entry("lucky_draw.rename")["changes"],
        json!([{"field": "name", "format": "text", "old": "Tri ân 20/11", "new": "Tri ân Tết"}])
    );
    let edited = entry("lucky_draw.participant_update");
    assert_eq!(edited["detail"], "Nguyễn Thị Hòa");
    assert_eq!(
        edited["changes"],
        json!([{"field": "name", "format": "text", "old": "Nguyễn Thị Hoa", "new": "Nguyễn Thị Hòa"}])
    );
    assert_eq!(
        entry("lucky_draw.participant_delete")["detail"],
        "Trần Văn Minh"
    );
    assert_eq!(
        entry("lucky_draw.participants_add")["changes"],
        json!([{"field": "participants_added", "format": "number", "new": "3"}])
    );
    assert_eq!(entry("lucky_draw.prize_create")["detail"], "Quà phụ");
    let prize_edit = entry("lucky_draw.prize_update");
    assert_eq!(prize_edit["detail"], "Quà lớn");
    assert_eq!(
        prize_edit["changes"],
        json!([
            {"field": "name", "format": "text", "old": "Quà", "new": "Quà lớn"},
            {"field": "eligible", "format": "code", "old": "all", "new": "female"},
            {"field": "quantity", "format": "number", "old": "2", "new": "1"},
        ])
    );
    assert_eq!(entry("lucky_draw.prize_delete")["detail"], "Quà phụ");
    // The add never lists who was added.
    let page_text = page.body.to_string();
    assert!(
        !page_text.contains(&text(&mai["name"])) || text(&winner["name"]) == text(&mai["name"])
    );
    Ok(())
}

fn p95(mut samples: Vec<std::time::Duration>) -> std::time::Duration {
    samples.sort();
    let i = (samples.len() * 95).div_ceil(100).saturating_sub(1);
    samples.get(i).copied().unwrap_or_default()
}

/// Spec §7 budgets, measured by hand (`cargo test -p app --test lucky_draw -- --ignored
/// --nocapture`): a draw of 2.000 people, 50 prizes and 1.000 results read in p95 < 150 ms, a spin
/// in p95 < 100 ms (local).
#[ignore = "bench: run by hand"]
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn bench_a_full_draw(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let draw = text(&s.draw("Tri ân lớn").await["id"]);
    sqlx::query(
        "INSERT INTO app.lucky_draw_participant (id, tenant_id, draw_id, name, gender, position, created_by)
         SELECT gen_random_uuid(), d.tenant_id, d.id, 'Người tham gia ' || g,
                CASE WHEN g % 2 = 0 THEN 'female' ELSE 'male' END, g, d.created_by
         FROM app.lucky_draw d, generate_series(1, 2000) g WHERE d.id = $1::uuid",
    )
    .bind(&draw)
    .execute(&s.h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.lucky_draw_prize (id, tenant_id, draw_id, name, eligible, quantity, position, created_by)
         SELECT gen_random_uuid(), d.tenant_id, d.id, 'Giải ' || g, 'all', 40, g, d.created_by
         FROM app.lucky_draw d, generate_series(1, 50) g WHERE d.id = $1::uuid",
    )
    .bind(&draw)
    .execute(&s.h.owner)
    .await?;
    sqlx::query(
        "INSERT INTO app.lucky_draw_result (id, tenant_id, draw_id, prize_id, participant_id, seq, drawn_by)
         SELECT gen_random_uuid(), p.tenant_id, p.draw_id, q.id, p.id, p.position, p.created_by
         FROM app.lucky_draw_participant p
         JOIN app.lucky_draw_prize q ON q.draw_id = p.draw_id AND q.position = (p.position % 50) + 1
         WHERE p.draw_id = $1::uuid AND p.position <= 1000",
    )
    .bind(&draw)
    .execute(&s.h.owner)
    .await?;
    sqlx::query("ANALYZE").execute(&s.h.owner).await?;
    let detail = s.get(&draw).await;
    assert_eq!(detail.body["results"].as_array().map(Vec::len), Some(1000));
    let prize = text(&detail.body["prizes"][0]["id"]);
    let mut reads = Vec::new();
    for _ in 0..50 {
        let t = std::time::Instant::now();
        assert_eq!(s.get(&draw).await.status, StatusCode::OK);
        reads.push(t.elapsed());
    }
    let mut spins = Vec::new();
    for _ in 0..20 {
        let t = std::time::Instant::now();
        assert_eq!(s.spin(&draw, &prize).await.status, StatusCode::CREATED);
        spins.push(t.elapsed());
    }
    println!(
        "GET /lucky-draws/{{id}} p95 = {:?}; POST …/spins p95 = {:?}",
        p95(reads),
        p95(spins)
    );
    Ok(())
}
