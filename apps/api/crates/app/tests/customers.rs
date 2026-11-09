//! S5: customer profiles & search (spec R1-1 US2, AC2.1–AC2.7).

mod common;

use axum::http::StatusCode;
use common::{
    OWNER_PHONE, TestResult, add_member, call, call_with_headers, harness, post_customer,
    session_for,
};
use identity::SystemRole;
use serde_json::{Value, json};
use sqlx::PgPool;

fn ha() -> Value {
    json!({"full_name": "Nguyễn Thị Hà", "phone": "0905 111 222", "gender": "female", "birth_year": 1985,
           "health_note": "Huyết áp cao — tránh đá nóng"})
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn creates_a_customer_and_finds_her_by_phone_or_accentless_name(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let created = post_customer(&h.app, Some(&owner), Some(ha())).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    assert_eq!(
        (
            created.body["phone"].clone(),
            created.body["phone_e164"].clone()
        ),
        (json!("0905 111 222"), json!("+84905111222"))
    );
    assert_eq!(created.body["health_note"], "Huyết áp cao — tránh đá nóng");

    // A whole number in any form `PhoneE164` accepts finds her (red before for "84905111222":
    // the 84 was dropped only after a '+', while the phone check accepts it without).
    for q in [
        "0905",
        "905111",
        "222",
        "0905 111 222",
        "+84 905 111 222",
        "84905111222",
        "ha",
        "nguyen thi",
        "Hà",
        "NGUYỄN",
    ] {
        let found = call(
            &h.app,
            "GET",
            &format!("/customers?q={}", urlencode(q)),
            Some(&owner),
            None,
        )
        .await;
        assert_eq!(found.status, StatusCode::OK, "{q}");
        assert_eq!(
            found.body.as_array().map(Vec::len),
            Some(1),
            "query {q:?} → {}",
            found.body
        );
        assert!(
            found.body[0].get("health_note").is_none(),
            "search results never carry notes"
        );
    }
    let too_short = call(&h.app, "GET", "/customers?q=h", Some(&owner), None).await;
    assert_eq!(too_short.status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn equivalent_phone_spellings_are_one_customer(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let first = post_customer(&h.app, Some(&owner), Some(ha())).await;
    let id = first.body["id"].clone();
    for phone in ["0905111222", "+84 905 111 222", "0905.111.222"] {
        let dup = post_customer(
            &h.app,
            Some(&owner),
            Some(json!({"full_name": "Ai đó", "phone": phone})),
        )
        .await;
        assert_eq!(
            (dup.status, dup.body["code"].as_str()),
            (StatusCode::CONFLICT, Some("customer.phone_taken")),
            "{phone}"
        );
        assert_eq!(
            dup.body["existing_id"], id,
            "AC2.4: points at the existing profile"
        );
    }
    let bad = post_customer(
        &h.app,
        Some(&owner),
        Some(json!({"full_name": "X", "phone": "12345"})),
    )
    .await;
    assert_eq!(
        bad.body["errors"][0],
        json!({"field": "phone", "code": "invalid_format"}),
        "AC2.5"
    );
    Ok(())
}

/// ADR-0019 D11: a double click (or a retry after a lost response) on "Tạo khách" sends the same
/// key twice and gets one profile. Red before: two anonymous walk-in profiles (no phone, so no
/// unique index stops the second).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn creating_a_customer_twice_with_one_key_makes_one_profile(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let key = uuid::Uuid::now_v7().to_string();
    let headers = [("idempotency-key", key.as_str())];
    let walk_in = json!({"gender": "female", "health_note": "Dị ứng tinh dầu"});
    let mut ids = Vec::new();
    for _ in 0..2 {
        let r = call_with_headers(
            &h.app,
            "POST",
            "/customers",
            Some(&owner),
            Some(walk_in.clone()),
            &headers,
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        assert_eq!(r.body["health_note"], "Dị ứng tinh dầu", "{}", r.body);
        ids.push(r.body["id"].clone());
    }
    assert_eq!(ids[0], ids[1], "the replay names the first profile");
    let profiles: i64 = sqlx::query_scalar("SELECT count(*) FROM app.customer")
        .fetch_one(&h.owner)
        .await?;
    assert_eq!(profiles, 1);
    // The record holds the id only: no health note, name or phone at rest in it.
    let stored: String =
        sqlx::query_scalar("SELECT response_body::text FROM app.idempotency_record")
            .fetch_one(&h.owner)
            .await?;
    assert!(!stored.contains("Dị ứng"), "{stored}");

    let other_body = call_with_headers(
        &h.app,
        "POST",
        "/customers",
        Some(&owner),
        Some(json!({"gender": "male"})),
        &headers,
    )
    .await;
    assert_eq!(
        (other_body.status, other_body.body["code"].as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    // No key at all (`post_customer` would add one).
    let no_key = call_with_headers(
        &h.app,
        "POST",
        "/customers",
        Some(&owner),
        Some(walk_in),
        &[],
    )
    .await;
    assert_eq!(
        (no_key.status, no_key.body["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("idempotency.key_required"))
    );
    Ok(())
}

/// F17: on POST /customers and POST /customers/{id}/follow-ups the right is checked before the
/// key and the body, so a technician hears 403 and learns nothing about what the route takes.
/// Red before: 400 idempotency.key_required.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn customer_writes_check_the_right_before_the_key_and_body(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        "0912000888",
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    let tech = session_for(&h, "0912000888").await;
    let id = uuid::Uuid::now_v7();
    for path in [
        "/customers".to_owned(),
        format!("/customers/{id}/follow-ups"),
    ] {
        let r = call(&h.app, "POST", &path, Some(&tech), Some(json!({"x": 1}))).await;
        assert_eq!(
            (r.status, r.body["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("auth.forbidden")),
            "{path}: {}",
            r.body
        );
    }
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn health_notes_are_encrypted_and_hidden_without_permission(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let created = post_customer(&h.app, Some(&owner), Some(ha())).await;
    let id = created.body["id"].as_str().unwrap_or_default().to_owned();

    let stored: Vec<u8> = sqlx::query_scalar("SELECT health_note_ct FROM app.customer")
        .fetch_one(&h.owner)
        .await?;
    assert!(
        !String::from_utf8_lossy(&stored).contains("Huyết"),
        "no plaintext at rest"
    );
    let audit: String = sqlx::query_scalar(
        "SELECT after::text FROM app.audit_log WHERE action = 'customer.create'",
    )
    .fetch_one(&h.owner)
    .await?;
    assert!(!audit.contains("Huyết"), "audit is redacted: {audit}");

    // A role that may read customers but not their health notes (owner customises a role).
    add_member(
        &h.owner,
        "vpyd",
        "0912000777",
        "Lễ tân",
        SystemRole::Manager,
    )
    .await?;
    sqlx::query("DELETE FROM app.role_permission WHERE permission LIKE 'customer.health_notes.%' AND role_id IN (SELECT id FROM app.role WHERE key = 'manager')")
        .execute(&h.owner)
        .await?;
    let reception = session_for(&h, "0912000777").await;
    let seen = call(
        &h.app,
        "GET",
        &format!("/customers/{id}"),
        Some(&reception),
        None,
    )
    .await;
    assert_eq!(seen.status, StatusCode::OK);
    assert!(
        seen.body.get("health_note").is_none(),
        "AC2.6: key absent, not just hidden: {}",
        seen.body
    );

    let write = post_customer(
        &h.app,
        Some(&reception),
        Some(json!({"full_name": "B", "phone": "0905999888", "health_note": "x"})),
    )
    .await;
    assert_eq!(
        write.status,
        StatusCode::FORBIDDEN,
        "writing a health note needs its own permission"
    );
    let set = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{id}/health-note"),
        Some(&reception),
        Some(json!({"health_note": "x"})),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(set.status, StatusCode::FORBIDDEN);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_updates_profile_and_health_note_with_optimistic_locking(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let created = post_customer(&h.app, Some(&owner), Some(ha())).await;
    let id = created.body["id"].as_str().unwrap_or_default().to_owned();

    let profile = json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female", "birth_year": 1985, "address": "Đông Hà", "source": null, "note": "Khách quen"});
    let updated = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{id}"),
        Some(&owner),
        Some(profile.clone()),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(
        (updated.status, updated.body["version"].clone()),
        (StatusCode::OK, json!(2))
    );
    assert_eq!(
        updated.body["health_note"], "Huyết áp cao — tránh đá nóng",
        "profile edit keeps the note"
    );

    let stale = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{id}"),
        Some(&owner),
        Some(profile),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);

    let cleared = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{id}/health-note"),
        Some(&owner),
        Some(json!({"health_note": null})),
        &[("if-match", "\"2\"")],
    )
    .await;
    assert_eq!(
        (cleared.status, cleared.body["health_note"].clone()),
        (StatusCode::OK, Value::Null)
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn decomposed_unicode_names_are_stored_composed_and_found(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let decomposed = "Le\u{0302} Thi\u{0323} A\u{0301}nh"; // "Lê Thị Ánh" with combining marks
    let created = post_customer(
        &h.app,
        Some(&owner),
        Some(json!({"full_name": decomposed, "phone": "0987654321"})),
    )
    .await;
    assert_eq!(created.body["full_name"], "Lê Thị Ánh", "AC2.7: NFC");
    let found = call(
        &h.app,
        "GET",
        &format!("/customers?q={}", urlencode("Lê Thị Ánh")),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(found.body.as_array().map(Vec::len), Some(1));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn technicians_and_other_spas_cannot_reach_customers(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let created = post_customer(&h.app, Some(&owner), Some(ha())).await;
    let id = created.body["id"].as_str().unwrap_or_default().to_owned();

    add_member(
        &h.owner,
        "vpyd",
        "0912000888",
        "KTV Lan",
        SystemRole::Technician,
    )
    .await?;
    let ktv = session_for(&h, "0912000888").await;
    assert_eq!(
        call(&h.app, "GET", "/customers?q=ha", Some(&ktv), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&h.app, "GET", &format!("/customers/{id}"), Some(&ktv), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    identity::provisioning::create_tenant(&h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_member(
        &h.owner,
        "spa-b",
        "0987000999",
        "Chủ Spa B",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&h, "0987000999").await;
    assert_eq!(
        call(
            &h.app,
            "GET",
            &format!("/customers/{id}"),
            Some(&other),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&h.app, "GET", "/customers?q=0905", Some(&other), None)
            .await
            .body,
        json!([])
    );
    // Spa B may register the same phone: uniqueness is per spa.
    let same_phone = post_customer(
        &h.app,
        Some(&other),
        Some(json!({"full_name": "Hà (spa B)", "phone": "0905111222"})),
    )
    .await;
    assert_eq!(same_phone.status, StatusCode::CREATED);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn search_stays_fast_with_ten_thousand_customers(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    sqlx::query(
        r"INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164)
          SELECT gen_random_uuid(), t.id, 'Khách ' || g, 'khach ' || g, '+849' || lpad(g::text, 8, '0')
          FROM generate_series(1, 10000) g, app.tenant t WHERE t.slug = 'vpyd'",
    )
    .execute(&h.owner)
    .await?;
    sqlx::query("ANALYZE app.customer")
        .execute(&h.owner)
        .await?;
    let owner = session_for(&h, OWNER_PHONE).await;

    let mut worst = std::time::Duration::ZERO;
    for q in ["0900001234", "khach 777", "00012"] {
        let started = std::time::Instant::now();
        let reply = call(
            &h.app,
            "GET",
            &format!("/customers?q={}", urlencode(q)),
            Some(&owner),
            None,
        )
        .await;
        worst = worst.max(started.elapsed());
        assert_eq!(reply.status, StatusCode::OK);
    }
    // Spec budget is 300 ms p95 in production; allow headroom for a debug build on a shared CI runner.
    assert!(
        worst < std::time::Duration::from_millis(800),
        "slowest search took {worst:?}"
    );
    Ok(())
}

/// ADR-0019 D13: the audit trail outlives an anonymised profile, so it never holds the name, phone
/// or birth date: only which fields are filled and which an edit changed (red before: the rows
/// carried all three in clear).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn customer_audit_keeps_who_and_what_field_but_no_personal_values(
    pool: PgPool,
) -> TestResult {
    let h = harness(pool).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let created = post_customer(
        &h.app,
        Some(&owner),
        Some(
            json!({"full_name": "Trần Thị Lụa", "phone": "0905 111 222", "gender": "female",
                    "birth_date": "1985-04-12"}),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let id = created.body["id"].as_str().unwrap_or_default().to_owned();
    let updated = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{id}"),
        Some(&owner),
        Some(
            json!({"full_name": "Trần Thị Lụa B", "phone": "0905 333 444", "gender": "female",
                    "birth_date": "1985-04-12", "note": "Khách quen"}),
        ),
        &[("if-match", "\"1\"")],
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);

    let rows: Vec<(String, Option<Value>, Option<Value>, bool)> = sqlx::query_as(
        "SELECT action, before, after, actor_user_id IS NOT NULL FROM app.audit_log
         WHERE entity_type = 'customer' AND entity_id = $1::uuid ORDER BY at, id",
    )
    .bind(&id)
    .fetch_all(&h.owner)
    .await?;
    let text = format!("{rows:?}");
    for value in ["Lụa", "905111222", "905333444", "1985"] {
        assert!(!text.contains(value), "audit holds {value:?}: {text}");
    }
    let [
        (create, None, Some(after_create), true),
        (update, Some(before), Some(after), true),
    ] = rows.as_slice()
    else {
        panic!("one create and one update, each naming its actor: {text}");
    };
    assert_eq!(
        (create.as_str(), update.as_str()),
        ("customer.create", "customer.update")
    );
    assert_eq!(
        (
            &after_create["has_name"],
            &after_create["has_phone"],
            &after_create["has_birth_date"]
        ),
        (&json!(true), &json!(true), &json!(true))
    );
    assert_eq!(before["has_phone"], json!(true));
    assert_eq!(after["changed"], json!(["full_name", "phone", "note"]));
    Ok(())
}

/// 0047 rewrites the customer audit rows written before D13 the same way, keeping who, when and
/// which field changed. Runs the migrations before 0047, plants rows in the old shape, then the rest.
#[sqlx::test(migrations = false)]
async fn migration_0047_scrubs_personal_values_from_old_customer_audit_rows(
    pool: PgPool,
) -> TestResult {
    kernel::MIGRATOR.run_to(46, &pool).await?;
    let tenant = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO app.tenant (id, name, slug) VALUES ($1, 'Spa', 'spa')")
        .bind(tenant)
        .execute(&pool)
        .await?;
    let old = [
        (
            "customer.create",
            None,
            json!({"full_name": "Lê Thị Mai", "phone_e164": "+84905111222", "gender": "female",
                   "birth_year": 1990, "birth_date": "1990-05-01", "health_note": "[redacted]"}),
        ),
        (
            "customer.update",
            Some(
                json!({"full_name": "Lê Thị Mai", "phone_e164": "+84905111222", "gender": "female",
                       "birth_year": null, "birth_date": null, "health_note": null}),
            ),
            json!({"full_name": "Lê Thị Mai", "phone_e164": "+84905333444", "gender": "female",
                   "birth_year": 1990, "birth_date": null, "health_note": null}),
        ),
    ];
    for (action, before, after) in &old {
        sqlx::query(
            "INSERT INTO app.audit_log (id, tenant_id, actor_kind, action, entity_type, entity_id, before, after)
             VALUES ($1, $2, 'system', $3, 'customer', $4, $5, $6)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(tenant)
        .bind(action)
        .bind(uuid::Uuid::now_v7())
        .bind(before)
        .bind(after)
        .execute(&pool)
        .await?;
    }
    kernel::MIGRATOR.run(&pool).await?;

    let rows: Vec<(String, Option<Value>, Value)> = sqlx::query_as(
        "SELECT action, before, after FROM app.audit_log WHERE entity_type = 'customer' ORDER BY action",
    )
    .fetch_all(&pool)
    .await?;
    let text = format!("{rows:?}");
    for value in ["Mai", "905111222", "905333444", "1990"] {
        assert!(!text.contains(value), "audit still holds {value:?}: {text}");
    }
    assert_eq!(
        rows,
        vec![
            (
                "customer.create".to_owned(),
                None,
                json!({"gender": "female", "has_name": true, "has_phone": true, "has_birth_year": true,
                       "has_birth_date": true, "has_health_note": true}),
            ),
            (
                "customer.update".to_owned(),
                Some(
                    json!({"gender": "female", "has_name": true, "has_phone": true,
                            "has_birth_year": false, "has_birth_date": false, "has_health_note": false})
                ),
                json!({"gender": "female", "has_name": true, "has_phone": true, "has_birth_year": true,
                       "has_birth_date": false, "has_health_note": false,
                       "changed": ["birth_year", "phone"]}),
            ),
        ]
    );
    Ok(())
}

/// ADR-0019 D12: the database refuses what `PhoneE164` refuses, a +84 number without a mobile
/// prefix (red before: the CHECKs took any +84 and 9 digits). Staff and appointment guests carry
/// the same CHECK (migration 0047).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn database_refuses_a_phone_without_a_mobile_prefix(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let tenant: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let owner = &h.owner;
    let insert_customer = move |phone: &'static str| {
        sqlx::query(
            "INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164)
             VALUES (gen_random_uuid(), $1, 'Khách', 'khach', $2)",
        )
        .bind(tenant)
        .bind(phone)
        .execute(owner)
    };
    let landline = "+84212345678";
    let customer = insert_customer(landline).await;
    let login = sqlx::query(
        "UPDATE app.user_account SET phone_e164 = $1 WHERE phone_e164 = '+84905123456'",
    )
    .bind(landline)
    .execute(&h.owner)
    .await;
    for (table, result) in [("customer", customer), ("user_account", login)] {
        let err = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("phone_mobile"), "{table}: {err:?}");
    }
    let mobile = insert_customer("+84355123456").await;
    assert!(mobile.is_ok(), "{mobile:?}");
    Ok(())
}
