//! SQL of the lucky draw. Every query names the tenant explicitly (RLS is the second layer) and a
//! child row is always reached through its draw (`draw_id` in the key), so an id of another
//! draw or spa matches nothing.

use chrono::{DateTime, Utc};
use kernel::TenantTx;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DrawRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct ParticipantRow {
    pub id: Uuid,
    pub name: String,
    pub gender: String,
    pub position: i32,
    pub version: i64,
    /// Their result, forfeited or not.
    pub result_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct PrizeRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub eligible: String,
    pub quantity: i32,
    pub position: i32,
    pub version: i64,
    /// Results not forfeited.
    pub awarded: i64,
    /// Every result, forfeited ones included (a prize with any is no longer deletable).
    pub results: i64,
}

#[derive(Debug, Clone)]
pub struct ResultRow {
    pub id: Uuid,
    pub seq: i32,
    pub prize_id: Uuid,
    pub participant_id: Uuid,
    pub drawn_at: DateTime<Utc>,
    pub forfeited_at: Option<DateTime<Utc>>,
}

/// One line of the list, before its state is derived.
#[derive(Debug, Clone)]
pub struct SummaryRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub participants: i64,
    pub results: i64,
    pub undrawn_female: i64,
    pub undrawn_male: i64,
}

/// A prize of some draw of the spa, for the list's derived state.
#[derive(Debug, Clone)]
pub struct PrizeTallyRow {
    pub draw_id: Uuid,
    pub eligible: String,
    pub quantity: i32,
    pub awarded: i64,
}

// ---------------------------------------------------------------- draws

pub async fn insert_draw(
    tx: &mut TenantTx,
    id: Uuid,
    name: &str,
    created_by: Uuid,
) -> Result<DrawRow, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        DrawRow,
        r#"INSERT INTO app.lucky_draw (id, tenant_id, name, created_by)
           VALUES ($1, $2, $3, $4)
           RETURNING id, name, created_at, version"#,
        id,
        tenant,
        name,
        created_by,
    )
    .fetch_one(tx.conn())
    .await
}

pub async fn get_draw(tx: &mut TenantTx, id: Uuid) -> Result<Option<DrawRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        DrawRow,
        r#"SELECT id, name, created_at, version FROM app.lucky_draw
           WHERE tenant_id = $1 AND id = $2"#,
        tenant,
        id,
    )
    .fetch_optional(tx.conn())
    .await
}

/// The draw, locked until the transaction ends: every write of a draw takes this first, so two
/// writes of one draw (two spins from two tabs, a spin and a delete) run one after the other.
pub async fn lock_draw(tx: &mut TenantTx, id: Uuid) -> Result<Option<DrawRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        DrawRow,
        r#"SELECT id, name, created_at, version FROM app.lucky_draw
           WHERE tenant_id = $1 AND id = $2
           FOR UPDATE"#,
        tenant,
        id,
    )
    .fetch_optional(tx.conn())
    .await
}

/// A new version of the draw (any write inside it); `name` renames it as well.
pub async fn touch_draw(
    tx: &mut TenantTx,
    id: Uuid,
    name: Option<&str>,
) -> Result<DrawRow, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        DrawRow,
        r#"UPDATE app.lucky_draw
           SET name = coalesce($3, name), version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND id = $2
           RETURNING id, name, created_at, version"#,
        tenant,
        id,
        name,
    )
    .fetch_one(tx.conn())
    .await
}

/// Deletes the draw and, by cascade, its participants, prizes and results.
pub async fn delete_draw(tx: &mut TenantTx, id: Uuid) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.lucky_draw WHERE tenant_id = $1 AND id = $2",
        tenant,
        id,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

/// Every draw of the spa, newest first, with its counts.
pub async fn summaries(tx: &mut TenantTx) -> Result<Vec<SummaryRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        SummaryRow,
        r#"SELECT d.id, d.name, d.created_at,
                  (SELECT count(*) FROM app.lucky_draw_participant p
                   WHERE p.tenant_id = d.tenant_id AND p.draw_id = d.id) AS "participants!",
                  (SELECT count(*) FROM app.lucky_draw_result r
                   WHERE r.tenant_id = d.tenant_id AND r.draw_id = d.id) AS "results!",
                  (SELECT count(*) FROM app.lucky_draw_participant p
                   WHERE p.tenant_id = d.tenant_id AND p.draw_id = d.id AND p.gender = 'female'
                     AND NOT EXISTS (SELECT 1 FROM app.lucky_draw_result r
                                     WHERE r.tenant_id = p.tenant_id AND r.draw_id = p.draw_id
                                       AND r.participant_id = p.id)) AS "undrawn_female!",
                  (SELECT count(*) FROM app.lucky_draw_participant p
                   WHERE p.tenant_id = d.tenant_id AND p.draw_id = d.id AND p.gender = 'male'
                     AND NOT EXISTS (SELECT 1 FROM app.lucky_draw_result r
                                     WHERE r.tenant_id = p.tenant_id AND r.draw_id = p.draw_id
                                       AND r.participant_id = p.id)) AS "undrawn_male!"
           FROM app.lucky_draw d
           WHERE d.tenant_id = $1
           ORDER BY d.created_at DESC, d.id DESC"#,
        tenant,
    )
    .fetch_all(tx.conn())
    .await
}

/// Every prize of the spa's draws with what it awarded, for the list's derived state.
pub async fn prize_tallies(tx: &mut TenantTx) -> Result<Vec<PrizeTallyRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        PrizeTallyRow,
        r#"SELECT q.draw_id, q.eligible, q.quantity,
                  (SELECT count(*) FROM app.lucky_draw_result r
                   WHERE r.tenant_id = q.tenant_id AND r.draw_id = q.draw_id AND r.prize_id = q.id
                     AND r.forfeited_at IS NULL) AS "awarded!"
           FROM app.lucky_draw_prize q
           WHERE q.tenant_id = $1"#,
        tenant,
    )
    .fetch_all(tx.conn())
    .await
}

// ---------------------------------------------------------------- participants

/// The draw's people in entry order, each with their result if any.
pub async fn participants(
    tx: &mut TenantTx,
    draw: Uuid,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        ParticipantRow,
        r#"SELECT p.id, p.name, p.gender, p.position, p.version, r.id AS "result_id?"
           FROM app.lucky_draw_participant p
           LEFT JOIN app.lucky_draw_result r
             ON r.tenant_id = p.tenant_id AND r.draw_id = p.draw_id AND r.participant_id = p.id
           WHERE p.tenant_id = $1 AND p.draw_id = $2
           ORDER BY p.position, p.id"#,
        tenant,
        draw,
    )
    .fetch_all(tx.conn())
    .await
}

pub async fn participant(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
) -> Result<Option<ParticipantRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        ParticipantRow,
        r#"SELECT p.id, p.name, p.gender, p.position, p.version, r.id AS "result_id?"
           FROM app.lucky_draw_participant p
           LEFT JOIN app.lucky_draw_result r
             ON r.tenant_id = p.tenant_id AND r.draw_id = p.draw_id AND r.participant_id = p.id
           WHERE p.tenant_id = $1 AND p.draw_id = $2 AND p.id = $3"#,
        tenant,
        draw,
        id,
    )
    .fetch_optional(tx.conn())
    .await
}

/// How many people the draw has, and the position the next one takes.
pub async fn participant_counts(tx: &mut TenantTx, draw: Uuid) -> Result<(i64, i32), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT count(*) AS "count!", coalesce(max(position), 0) + 1 AS "next!"
           FROM app.lucky_draw_participant WHERE tenant_id = $1 AND draw_id = $2"#,
        tenant,
        draw,
    )
    .fetch_one(tx.conn())
    .await?;
    Ok((row.count, row.next))
}

/// Inserts people in one statement; `ids`, `names`, `genders`, `positions` are parallel.
pub async fn insert_participants(
    tx: &mut TenantTx,
    draw: Uuid,
    ids: &[Uuid],
    names: &[String],
    genders: &[String],
    positions: &[i32],
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.lucky_draw_participant
             (id, tenant_id, draw_id, name, gender, position, created_by)
           SELECT i, $1, $2, n, g, p, $7
           FROM unnest($3::uuid[], $4::text[], $5::text[], $6::int4[]) AS t(i, n, g, p)"#,
        tenant,
        draw,
        ids,
        names,
        genders,
        positions,
        created_by,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

pub async fn update_participant(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
    name: &str,
    gender: &str,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.lucky_draw_participant
           SET name = $4, gender = $5, version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND draw_id = $2 AND id = $3"#,
        tenant,
        draw,
        id,
        name,
        gender,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

pub async fn delete_participant(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.lucky_draw_participant WHERE tenant_id = $1 AND draw_id = $2 AND id = $3",
        tenant,
        draw,
        id,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

/// The people still in the draw who may win a prize for `eligible` (`all`, `female`, `male`), in
/// entry order: the list the winner is picked from.
pub async fn eligible_participants(
    tx: &mut TenantTx,
    draw: Uuid,
    eligible: &str,
) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query!(
        r#"SELECT p.id, p.name FROM app.lucky_draw_participant p
           WHERE p.tenant_id = $1 AND p.draw_id = $2 AND ($3 = 'all' OR p.gender = $3)
             AND NOT EXISTS (SELECT 1 FROM app.lucky_draw_result r
                             WHERE r.tenant_id = p.tenant_id AND r.draw_id = p.draw_id
                               AND r.participant_id = p.id)
           ORDER BY p.position, p.id"#,
        tenant,
        draw,
        eligible,
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.name)).collect())
}

// ---------------------------------------------------------------- prizes

/// The draw's prizes in order, each with what it awarded.
pub async fn prizes(tx: &mut TenantTx, draw: Uuid) -> Result<Vec<PrizeRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        PrizeRow,
        r#"SELECT q.id, q.name, q.description, q.eligible, q.quantity, q.position, q.version,
                  count(r.id) FILTER (WHERE r.forfeited_at IS NULL) AS "awarded!",
                  count(r.id) AS "results!"
           FROM app.lucky_draw_prize q
           LEFT JOIN app.lucky_draw_result r
             ON r.tenant_id = q.tenant_id AND r.draw_id = q.draw_id AND r.prize_id = q.id
           WHERE q.tenant_id = $1 AND q.draw_id = $2
           GROUP BY q.id
           ORDER BY q.position, q.id"#,
        tenant,
        draw,
    )
    .fetch_all(tx.conn())
    .await
}

pub async fn prize(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
) -> Result<Option<PrizeRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        PrizeRow,
        r#"SELECT q.id, q.name, q.description, q.eligible, q.quantity, q.position, q.version,
                  count(r.id) FILTER (WHERE r.forfeited_at IS NULL) AS "awarded!",
                  count(r.id) AS "results!"
           FROM app.lucky_draw_prize q
           LEFT JOIN app.lucky_draw_result r
             ON r.tenant_id = q.tenant_id AND r.draw_id = q.draw_id AND r.prize_id = q.id
           WHERE q.tenant_id = $1 AND q.draw_id = $2 AND q.id = $3
           GROUP BY q.id"#,
        tenant,
        draw,
        id,
    )
    .fetch_optional(tx.conn())
    .await
}

/// How many prizes the draw has, and the position the next one takes.
pub async fn prize_counts(tx: &mut TenantTx, draw: Uuid) -> Result<(i64, i32), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT count(*) AS "count!", coalesce(max(position), 0) + 1 AS "next!"
           FROM app.lucky_draw_prize WHERE tenant_id = $1 AND draw_id = $2"#,
        tenant,
        draw,
    )
    .fetch_one(tx.conn())
    .await?;
    Ok((row.count, row.next))
}

/// What a prize is: name, description, who it is for, how many slots.
pub struct PrizeFields<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub eligible: &'a str,
    pub quantity: i32,
}

pub async fn insert_prize(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
    position: i32,
    f: &PrizeFields<'_>,
    created_by: Uuid,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"INSERT INTO app.lucky_draw_prize
             (id, tenant_id, draw_id, name, description, eligible, quantity, position, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        id,
        tenant,
        draw,
        f.name,
        f.description,
        f.eligible,
        f.quantity,
        position,
        created_by,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

pub async fn update_prize(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
    f: &PrizeFields<'_>,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.lucky_draw_prize
           SET name = $4, description = $5, eligible = $6, quantity = $7,
               version = version + 1, updated_at = now()
           WHERE tenant_id = $1 AND draw_id = $2 AND id = $3"#,
        tenant,
        draw,
        id,
        f.name,
        f.description,
        f.eligible,
        f.quantity,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

pub async fn delete_prize(tx: &mut TenantTx, draw: Uuid, id: Uuid) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        "DELETE FROM app.lucky_draw_prize WHERE tenant_id = $1 AND draw_id = $2 AND id = $3",
        tenant,
        draw,
        id,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}

// ---------------------------------------------------------------- results

/// The draw's results in spin order.
pub async fn results(tx: &mut TenantTx, draw: Uuid) -> Result<Vec<ResultRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        ResultRow,
        r#"SELECT id, seq, prize_id, participant_id, drawn_at, forfeited_at
           FROM app.lucky_draw_result
           WHERE tenant_id = $1 AND draw_id = $2
           ORDER BY seq"#,
        tenant,
        draw,
    )
    .fetch_all(tx.conn())
    .await
}

pub async fn result(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
) -> Result<Option<ResultRow>, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        ResultRow,
        r#"SELECT id, seq, prize_id, participant_id, drawn_at, forfeited_at
           FROM app.lucky_draw_result
           WHERE tenant_id = $1 AND draw_id = $2 AND id = $3"#,
        tenant,
        draw,
        id,
    )
    .fetch_optional(tx.conn())
    .await
}

/// A new result with the next spin number of the draw (the caller holds the draw's lock, so no
/// other spin takes the same number). `drawn_at` is the write time, so it follows `seq`.
pub async fn insert_result(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
    prize: Uuid,
    participant: Uuid,
    drawn_by: Uuid,
) -> Result<ResultRow, sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query_as!(
        ResultRow,
        r#"INSERT INTO app.lucky_draw_result
             (id, tenant_id, draw_id, prize_id, participant_id, seq, drawn_at, drawn_by)
           SELECT $1, $2, $3, $4, $5, coalesce(max(seq), 0) + 1, clock_timestamp(), $6
           FROM app.lucky_draw_result WHERE tenant_id = $2 AND draw_id = $3
           RETURNING id, seq, prize_id, participant_id, drawn_at, forfeited_at"#,
        id,
        tenant,
        draw,
        prize,
        participant,
        drawn_by,
    )
    .fetch_one(tx.conn())
    .await
}

pub async fn forfeit_result(
    tx: &mut TenantTx,
    draw: Uuid,
    id: Uuid,
    by: Uuid,
) -> Result<(), sqlx::Error> {
    let tenant = tx.tenant().as_uuid();
    sqlx::query!(
        r#"UPDATE app.lucky_draw_result
           SET forfeited_at = clock_timestamp(), forfeited_by = $4
           WHERE tenant_id = $1 AND draw_id = $2 AND id = $3 AND forfeited_at IS NULL"#,
        tenant,
        draw,
        id,
        by,
    )
    .execute(tx.conn())
    .await
    .map(|_| ())
}
