//! Use cases of the lucky draw (spec lucky-draw US1–US6). Every one requires `lucky_draw.manage`
//! (the owner) before touching data and runs in one `TenantTx`. Every write of a draw locks the
//! draw's row first ([`repo::lock_draw`]), bumps its `version`, writes its audit entry and answers
//! with the whole draw as it is after the change ([`Detail`]), read in the same transaction.
//!
//! The winner of a spin is picked here, from the people still eligible, with a random source the
//! caller hands in ([`RandomSource`]; the OS CSPRNG in production): the client sends a prize,
//! never a person or a number (ADR-0025).

use crate::domain::{
    self, Eligible, Gender, PrizeTally, RandomSource, Status, Undrawn, uniform_index,
};
use crate::repo::{self, DrawRow, ParticipantRow, PrizeFields, PrizeRow, ResultRow};
use kernel::audit::{self, Action, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

const ENTITY: &str = "lucky_draw";

// ---------------------------------------------------------------- what the use cases answer

/// A prize with what is left of it.
#[derive(Debug, Clone)]
pub struct PrizeView {
    pub row: PrizeRow,
    pub tally: PrizeTally,
}

/// A whole draw, as every read and write answers it.
#[derive(Debug, Clone)]
pub struct Detail {
    pub draw: DrawRow,
    pub status: Status,
    pub participants: Vec<ParticipantRow>,
    pub prizes: Vec<PrizeView>,
    pub results: Vec<ResultRow>,
}

/// One line of the list.
#[derive(Debug, Clone)]
pub struct Summary {
    pub id: Uuid,
    pub name: String,
    pub created_at: jiff::Timestamp,
    pub participant_count: i64,
    /// Every prize's quantity, summed.
    pub prize_slots: i64,
    /// Results not forfeited.
    pub awarded: i64,
    pub status: Status,
}

/// One person to add.
#[derive(Debug, Clone)]
pub struct ParticipantInput {
    pub name: String,
    pub gender: Gender,
}

/// A prize as written (created or replaced).
#[derive(Debug, Clone)]
pub struct PrizeInput {
    pub name: String,
    pub description: Option<String>,
    pub eligible: Eligible,
    pub quantity: i32,
}

fn count(n: i64) -> u32 {
    u32::try_from(n.max(0)).unwrap_or(u32::MAX)
}

/// A stored code the column CHECK allows; anything else is a corrupted row, never a guess.
fn stored<T>(parsed: Option<T>, what: &str) -> Result<T, AppError> {
    parsed.ok_or_else(|| AppError::Internal(format!("unknown stored {what}")))
}

fn eligible_of(row: &PrizeRow) -> Result<Eligible, AppError> {
    stored(Eligible::parse(&row.eligible), "prize eligibility")
}

/// The draw as it is now, inside `tx`.
async fn detail(tx: &mut TenantTx, draw: DrawRow) -> Result<Detail, AppError> {
    let participants = repo::participants(tx, draw.id).await?;
    let prizes = repo::prizes(tx, draw.id).await?;
    let results = repo::results(tx, draw.id).await?;
    let undrawn = Undrawn::of(
        participants
            .iter()
            .filter(|p| p.result_id.is_none())
            .map(|p| stored(Gender::parse(&p.gender), "participant gender"))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let prizes = prizes
        .into_iter()
        .map(|row| {
            let tally = PrizeTally {
                quantity: count(i64::from(row.quantity)),
                awarded: count(row.awarded),
                eligible_count: eligible_of(&row)?.eligible_count(undrawn),
            };
            Ok(PrizeView { row, tally })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let status = Status::of(results.len(), prizes.iter().map(|p| p.tally));
    Ok(Detail {
        draw,
        status,
        participants,
        prizes,
        results,
    })
}

async fn locked(tx: &mut TenantTx, id: Uuid) -> Result<DrawRow, AppError> {
    repo::lock_draw(tx, id).await?.ok_or(AppError::NotFound)
}

async fn record(
    tx: &mut TenantTx,
    actor: &Actor,
    action: Action,
    draw: Uuid,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<(), AppError> {
    audit::record(
        tx,
        actor,
        AuditEntry {
            action,
            entity_type: ENTITY,
            entity_id: draw,
            before,
            after,
            reason: None,
        },
    )
    .await
}

/// The end of every write inside a draw: a new version, the audit entry, the draw as it is now.
async fn finish(
    mut tx: TenantTx,
    actor: &Actor,
    draw: Uuid,
    action: Action,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<Detail, AppError> {
    let row = repo::touch_draw(&mut tx, draw, None).await?;
    record(&mut tx, actor, action, draw, before, after).await?;
    let d = detail(&mut tx, row).await?;
    tx.commit().await?;
    Ok(d)
}

fn fail_if_any(errors: Vec<FieldError>) -> Result<(), AppError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Validation(errors))
    }
}

// ---------------------------------------------------------------- draws (US1)

/// `GET /lucky-draws`: every draw of the spa, newest first.
pub async fn list(pool: &PgPool, actor: &Actor) -> Result<Vec<Summary>, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let rows = repo::summaries(&mut tx).await?;
    let tallies = repo::prize_tallies(&mut tx).await?;
    tx.commit().await?;
    let mut by_draw: HashMap<Uuid, Vec<repo::PrizeTallyRow>> = HashMap::new();
    for t in tallies {
        by_draw.entry(t.draw_id).or_default().push(t);
    }
    rows.into_iter()
        .map(|r| {
            let undrawn = Undrawn {
                female: count(r.undrawn_female),
                male: count(r.undrawn_male),
            };
            let prizes = by_draw.remove(&r.id).unwrap_or_default();
            let tallies = prizes
                .iter()
                .map(|q| {
                    Ok(PrizeTally {
                        quantity: count(i64::from(q.quantity)),
                        awarded: count(q.awarded),
                        eligible_count: stored(Eligible::parse(&q.eligible), "prize eligibility")?
                            .eligible_count(undrawn),
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(Summary {
                id: r.id,
                name: r.name,
                created_at: r.created_at,
                participant_count: r.participants,
                prize_slots: prizes.iter().map(|q| i64::from(q.quantity)).sum(),
                awarded: prizes.iter().map(|q| q.awarded).sum(),
                status: Status::of(usize::try_from(r.results).unwrap_or(0), tallies),
            })
        })
        .collect()
}

fn draw_name(name: &str) -> Result<String, AppError> {
    let mut errors = Vec::new();
    let name = domain::required_text(name, "name", domain::DRAW_NAME_MAX, &mut errors);
    fail_if_any(errors)?;
    Ok(name)
}

/// `POST /lucky-draws`.
pub async fn create(pool: &PgPool, actor: &Actor, name: &str) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let name = draw_name(name)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let row = repo::insert_draw(&mut tx, Uuid::now_v7(), &name, actor.user_id.as_uuid()).await?;
    record(
        &mut tx,
        actor,
        Action::LuckyDrawCreate,
        row.id,
        None,
        Some(json!({"name": row.name})),
    )
    .await?;
    let d = detail(&mut tx, row).await?;
    tx.commit().await?;
    Ok(d)
}

/// `GET /lucky-draws/{id}`.
pub async fn get(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let row = repo::get_draw(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let d = detail(&mut tx, row).await?;
    tx.commit().await?;
    Ok(d)
}

/// `PUT /lucky-draws/{id}` with `If-Match` = the draw's version.
pub async fn rename(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    version: i64,
    name: &str,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let name = draw_name(name)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let current = locked(&mut tx, id).await?;
    if current.version != version {
        return Err(AppError::StaleVersion);
    }
    let row = repo::touch_draw(&mut tx, id, Some(&name)).await?;
    record(
        &mut tx,
        actor,
        Action::LuckyDrawRename,
        id,
        Some(json!({"name": current.name})),
        Some(json!({"name": row.name})),
    )
    .await?;
    let d = detail(&mut tx, row).await?;
    tx.commit().await?;
    Ok(d)
}

/// `DELETE /lucky-draws/{id}`: the draw with its people, prizes and results (no undo, spec L7).
/// The trail keeps the name and the counts, never the people's names (spec AC1.4).
pub async fn delete(pool: &PgPool, actor: &Actor, id: Uuid) -> Result<(), AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let row = locked(&mut tx, id).await?;
    let d = detail(&mut tx, row).await?;
    repo::delete_draw(&mut tx, id).await?;
    record(
        &mut tx,
        actor,
        Action::LuckyDrawDelete,
        id,
        Some(json!({
            "name": d.draw.name,
            "participants": d.participants.len(),
            "prizes": d.prizes.len(),
            "results": d.results.len(),
        })),
        None,
    )
    .await?;
    tx.commit().await
}

// ---------------------------------------------------------------- participants (US2)

/// `POST /lucky-draws/{id}/participants`: 1–500 people, all or none (one transaction), at most
/// 2.000 in the draw. Entry order follows the list.
pub async fn add_participants(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    people: Vec<ParticipantInput>,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let (existing, next) = repo::participant_counts(&mut tx, id).await?;
    let total = usize::try_from(existing).unwrap_or(usize::MAX);
    if total.saturating_add(people.len()) > domain::PARTICIPANTS_MAX {
        return Err(AppError::Validation(vec![FieldError::new(
            "participants",
            "too_many",
        )]));
    }
    let ids: Vec<Uuid> = people.iter().map(|_| Uuid::now_v7()).collect();
    let names: Vec<String> = people.iter().map(|p| p.name.clone()).collect();
    let genders: Vec<String> = people
        .iter()
        .map(|p| p.gender.as_str().to_owned())
        .collect();
    let positions: Vec<i32> = (0..people.len())
        .map(|i| next.saturating_add(i32::try_from(i).unwrap_or(i32::MAX)))
        .collect();
    repo::insert_participants(
        &mut tx,
        id,
        &ids,
        &names,
        &genders,
        &positions,
        actor.user_id.as_uuid(),
    )
    .await?;
    let added = people.len();
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawParticipantsAdd,
        None,
        Some(json!({"added": added, "total": total + added})),
    )
    .await
}

fn person(row: &ParticipantRow) -> Value {
    json!({"participant_id": row.id, "name": row.name, "gender": row.gender})
}

/// Someone with a result (forfeited or not) stays as entered: the result names them.
fn not_drawn(row: ParticipantRow) -> Result<ParticipantRow, AppError> {
    if row.result_id.is_some() {
        return Err(AppError::Conflict {
            code: "lucky_draw.participant_drawn",
        });
    }
    Ok(row)
}

/// `PUT /lucky-draws/{id}/participants/{pid}` with `If-Match` = the person's version.
pub async fn update_participant(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    participant: Uuid,
    version: i64,
    input: ParticipantInput,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let row = repo::participant(&mut tx, id, participant)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.version != version {
        return Err(AppError::StaleVersion);
    }
    let row = not_drawn(row)?;
    repo::update_participant(&mut tx, id, participant, &input.name, input.gender.as_str()).await?;
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawParticipantUpdate,
        Some(person(&row)),
        Some(json!({
            "participant_id": participant, "name": input.name, "gender": input.gender.as_str(),
        })),
    )
    .await
}

/// `DELETE /lucky-draws/{id}/participants/{pid}`: only someone without a result.
pub async fn delete_participant(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    participant: Uuid,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let row = repo::participant(&mut tx, id, participant)
        .await?
        .ok_or(AppError::NotFound)
        .and_then(not_drawn)?;
    repo::delete_participant(&mut tx, id, participant).await?;
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawParticipantDelete,
        Some(person(&row)),
        None,
    )
    .await
}

// ---------------------------------------------------------------- prizes (US3)

fn prize_value(id: Uuid, name: &str, description: Option<&str>, eligible: &str, q: i32) -> Value {
    json!({
        "prize_id": id, "name": name, "description": description, "eligible": eligible,
        "quantity": q,
    })
}

fn fields(input: &PrizeInput) -> PrizeFields<'_> {
    PrizeFields {
        name: &input.name,
        description: input.description.as_deref(),
        eligible: input.eligible.as_str(),
        quantity: input.quantity,
    }
}

/// `POST /lucky-draws/{id}/prizes`: at most 50 per draw, appended to the order.
pub async fn create_prize(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    input: PrizeInput,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let (existing, next) = repo::prize_counts(&mut tx, id).await?;
    if usize::try_from(existing).unwrap_or(usize::MAX) >= domain::PRIZES_MAX {
        return Err(AppError::Validation(vec![FieldError::new(
            "prizes", "too_many",
        )]));
    }
    let prize = Uuid::now_v7();
    repo::insert_prize(
        &mut tx,
        id,
        prize,
        next,
        &fields(&input),
        actor.user_id.as_uuid(),
    )
    .await?;
    let after = prize_value(
        prize,
        &input.name,
        input.description.as_deref(),
        input.eligible.as_str(),
        input.quantity,
    );
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawPrizeCreate,
        None,
        Some(after),
    )
    .await
}

/// `PUT /lucky-draws/{id}/prizes/{qid}` with `If-Match` = the prize's version. Name and
/// description always; who it is for only while it has no result (forfeited ones count); the
/// quantity never below what it awarded.
pub async fn update_prize(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    prize: Uuid,
    version: i64,
    input: PrizeInput,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let row = repo::prize(&mut tx, id, prize)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.version != version {
        return Err(AppError::StaleVersion);
    }
    if row.results > 0 && eligible_of(&row)? != input.eligible {
        return Err(AppError::Conflict {
            code: "lucky_draw.prize_drawn",
        });
    }
    if i64::from(input.quantity) < row.awarded {
        return Err(AppError::Unprocessable {
            code: "lucky_draw.quantity_below_awarded",
        });
    }
    repo::update_prize(&mut tx, id, prize, &fields(&input)).await?;
    let before = prize_value(
        prize,
        &row.name,
        row.description.as_deref(),
        &row.eligible,
        row.quantity,
    );
    let after = prize_value(
        prize,
        &input.name,
        input.description.as_deref(),
        input.eligible.as_str(),
        input.quantity,
    );
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawPrizeUpdate,
        Some(before),
        Some(after),
    )
    .await
}

/// `DELETE /lucky-draws/{id}/prizes/{qid}`: only a prize without any result.
pub async fn delete_prize(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    prize: Uuid,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let row = repo::prize(&mut tx, id, prize)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.results > 0 {
        return Err(AppError::Conflict {
            code: "lucky_draw.prize_drawn",
        });
    }
    repo::delete_prize(&mut tx, id, prize).await?;
    let before = prize_value(
        prize,
        &row.name,
        row.description.as_deref(),
        &row.eligible,
        row.quantity,
    );
    finish(
        tx,
        actor,
        id,
        Action::LuckyDrawPrizeDelete,
        Some(before),
        None,
    )
    .await
}

// ---------------------------------------------------------------- spins (US4) and forfeits (US5)

/// `POST /lucky-draws/{id}/spins`, idempotent on `key`: draws a winner of `prize` among the
/// people still eligible, uniformly with `rng`, and records it as the draw's next spin. A retry
/// with the same key and body answers with the result the first request recorded, read again (the
/// idempotency record keeps only its id, never a name). Answers the result and the whole draw.
pub async fn spin(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    id: Uuid,
    prize: Uuid,
    rng: &mut dyn RandomSource,
) -> Result<(ResultRow, Detail), AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { body, .. } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        let result = body["result_id"]
            .as_str()
            .and_then(|r| Uuid::parse_str(r).ok())
            .ok_or_else(|| AppError::Internal("spin replay without a result id".into()))?;
        // Gone with its draw since: nothing to answer with.
        let row = repo::get_draw(&mut tx, id)
            .await?
            .ok_or(AppError::NotFound)?;
        let result = repo::result(&mut tx, id, result)
            .await?
            .ok_or(AppError::NotFound)?;
        let d = detail(&mut tx, row).await?;
        tx.commit().await?;
        return Ok((result, d));
    }
    locked(&mut tx, id).await?;
    let prize_row = repo::prize(&mut tx, id, prize)
        .await?
        .ok_or(AppError::NotFound)?;
    if i64::from(prize_row.quantity) <= prize_row.awarded {
        return Err(AppError::Conflict {
            code: "lucky_draw.prize_exhausted",
        });
    }
    let pool_of = repo::eligible_participants(&mut tx, id, &prize_row.eligible).await?;
    let pick = uniform_index(pool_of.len(), rng)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .and_then(|i| pool_of.get(i))
        .ok_or(AppError::Conflict {
            code: "lucky_draw.no_eligible",
        })?;
    let (winner, winner_name) = pick;
    let result = repo::insert_result(
        &mut tx,
        id,
        Uuid::now_v7(),
        prize,
        *winner,
        actor.user_id.as_uuid(),
    )
    .await?;
    idempotency::complete(&mut tx, actor, key, 201, &json!({"result_id": result.id})).await?;
    let after = json!({
        "result_id": result.id, "seq": result.seq,
        "prize_id": prize, "prize": prize_row.name,
        "participant_id": winner, "winner": winner_name,
    });
    let d = finish(tx, actor, id, Action::LuckyDrawSpin, None, Some(after)).await?;
    Ok((result, d))
}

/// `POST /lucky-draws/{id}/results/{rid}/forfeit`: the winner was not there. The slot goes back to
/// the prize; the person stays out of the draw (spec L3). Once only.
pub async fn forfeit(
    pool: &PgPool,
    actor: &Actor,
    id: Uuid,
    result: Uuid,
) -> Result<Detail, AppError> {
    actor.require(Permission::LuckyDrawManage)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    locked(&mut tx, id).await?;
    let row = repo::result(&mut tx, id, result)
        .await?
        .ok_or(AppError::NotFound)?;
    if row.forfeited_at.is_some() {
        return Err(AppError::Conflict {
            code: "lucky_draw.result_forfeited",
        });
    }
    let prize = repo::prize(&mut tx, id, row.prize_id).await?;
    let winner = repo::participant(&mut tx, id, row.participant_id).await?;
    repo::forfeit_result(&mut tx, id, result, actor.user_id.as_uuid()).await?;
    let after = json!({
        "result_id": result, "seq": row.seq,
        "prize_id": row.prize_id, "prize": prize.map(|p| p.name),
        "participant_id": row.participant_id, "winner": winner.map(|w| w.name),
    });
    finish(tx, actor, id, Action::LuckyDrawForfeit, None, Some(after)).await
}
