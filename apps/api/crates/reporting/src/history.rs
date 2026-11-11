//! "Ai ghi, ai sửa" on each record (ADR-0022, spec record-history, architecture §4): the history of
//! one visit, package or customer, read from `app.audit_log` with the same row reader and the same
//! one-line-per-action folding as the owner's trail ([`crate::activity`]).
//!
//! Authorization, in this order (F17): `record.history.read`, the route's own read permission
//! ([`require_route`]), then the record is re-anchored (a sale → its visit or its package, a
//! refund → its package; `404` when it is not the spa's) and the read permission of the anchor's
//! kind is required ([`require_anchor`], exhaustive), all before any history row is read.
//!
//! Membership is by audit row id (spec D6): each kind selects exactly the rows that are about it,
//! never "the same instant" or "the same entity" in general, so a package's later life never
//! leaks into the visit that sold it. What a line shows follows the caller's rights on the
//! original screens (D7, D8): money with `sale.read`, note changes with
//! `customer.health_notes.read`, a phone's values with `audit.read`.

use crate::activity::{self, Row};
use crate::activity_dto::{ActivityActor, ActivityEntry, ActorKind};
use crate::history_dto::{
    RecordCreated, RecordHistory, RecordKind, RecordLast, RecordRef, RecordSource, RecordState,
    RecordSummary,
};
use chrono::{DateTime, NaiveDate, Utc};
use kernel::audit::Action;
use kernel::{Actor, AppError, Permission, TenantTx};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

/// Newest lines kept (older ones are counted in `omitted`).
const MAX_LINES: usize = 100;
/// Longest "Sửa lượt" chain followed each way.
const MAX_CHAIN: i32 = 20;

/// The record a route names, before re-anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordRoute {
    Visit,
    Sale,
    Refund,
    CustomerPackage,
    Customer,
}

/// The record whose history is read, after re-anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Visit(Uuid),
    Package(Uuid),
    Customer(Uuid),
}

/// The rights of the route itself, before its id is looked at: the switch, then what the route
/// reads (a sale or refund route always needs `sale.read`).
pub fn require_route(actor: &Actor, route: RecordRoute) -> Result<(), AppError> {
    use Permission as P;
    actor.require(P::RecordHistoryRead)?;
    match route {
        RecordRoute::Visit => actor.require(P::VisitRead),
        RecordRoute::Sale => actor.require(P::SaleRead),
        RecordRoute::Refund => {
            actor.require(P::SaleRead)?;
            actor.require(P::CustomerRead)
        }
        RecordRoute::CustomerPackage | RecordRoute::Customer => actor.require(P::CustomerRead),
    }
}

/// The read right of the anchor's kind (B1): a sale of one-off services is its visit's history
/// (`visit.read`), a package sale or a refund its package's (`customer.read`).
fn require_anchor(actor: &Actor, anchor: Anchor) -> Result<(), AppError> {
    match anchor {
        Anchor::Visit(_) => actor.require(Permission::VisitRead),
        Anchor::Package(_) | Anchor::Customer(_) => actor.require(Permission::CustomerRead),
    }
}

/// The anchor of a requested record of the spa; `404` when there is none.
async fn resolve(tx: &mut TenantTx, route: RecordRoute, id: Uuid) -> Result<Anchor, AppError> {
    let tenant = tx.tenant().as_uuid();
    let anchor = match route {
        RecordRoute::Visit => sqlx::query_scalar!(
            "SELECT id FROM app.visit WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Visit),
        RecordRoute::Sale => {
            // A package sale is the package's (the revenue book's predicate: a `package` line,
            // migration 0021); `customer_package.sale_id` only finds which package.
            let sale = sqlx::query!(
                r#"SELECT s.visit_id,
                          EXISTS (SELECT 1 FROM app.sale_line sl
                                  WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
                                    AND sl.line_type = 'package') AS "is_package!",
                          (SELECT cp.id FROM app.customer_package cp
                           WHERE cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
                           ORDER BY cp.id LIMIT 1) AS package_id
                   FROM app.sale s WHERE s.tenant_id = $1 AND s.id = $2"#,
                tenant,
                id
            )
            .fetch_optional(tx.conn())
            .await?;
            sale.and_then(|s| {
                if s.is_package {
                    s.package_id.map(Anchor::Package)
                } else {
                    s.visit_id.map(Anchor::Visit)
                }
            })
        }
        RecordRoute::Refund => sqlx::query_scalar!(
            "SELECT customer_package_id FROM app.refund WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Package),
        RecordRoute::CustomerPackage => sqlx::query_scalar!(
            "SELECT id FROM app.customer_package WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Package),
        RecordRoute::Customer => sqlx::query_scalar!(
            "SELECT id FROM app.customer WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Customer),
    };
    anchor.ok_or(AppError::NotFound)
}

/// What the history of one anchor is made of.
struct Selected {
    kind: RecordKind,
    /// Rows of the history (spec D6).
    members: HashSet<Uuid>,
    /// Rows that may only fold into a member line (a package sold in a visit's checkout), never a
    /// line of their own.
    extra: Vec<Uuid>,
    /// The creating action's row, by its action and entity (D12).
    creation: Creation,
    /// Without a creating row: who / when from the record itself.
    fallback: Option<(Option<Uuid>, DateTime<Utc>)>,
    /// A paper-book package: its book's day.
    paper_day: Option<NaiveDate>,
    state: RecordState,
}

/// Which row is the creation.
enum Creation {
    /// `visit.create` of the chain's first visit.
    Visit(Uuid),
    /// `customer_package.create` of the package or the import that named it.
    Package(Uuid),
    /// `customer.create`, or an import that created the customer.
    Customer,
}

impl Creation {
    fn is(&self, row: &Row) -> bool {
        match (self, Action::parse(&row.action)) {
            (Creation::Visit(id), Some(Action::VisitCreate)) => row.entity_id == *id,
            (Creation::Package(id), Some(Action::CustomerPackageCreate)) => row.entity_id == *id,
            (Creation::Package(_), Some(Action::OpeningBalanceImport))
            | (Creation::Customer, Some(Action::CustomerCreate | Action::OpeningBalanceImport)) => {
                true
            }
            _ => false,
        }
    }
}

/// A visit's "Sửa lượt" chain: up the replaced visits, down the replacing ones.
async fn visit(tx: &mut TenantTx, id: Uuid) -> Result<Selected, AppError> {
    let tenant = tx.tenant().as_uuid();
    let chain = sqlx::query!(
        r#"WITH RECURSIVE up (id, corrects, depth) AS (
             SELECT v.id, v.corrects_visit_id, 1 FROM app.visit v WHERE v.tenant_id = $1 AND v.id = $2
             UNION ALL
             SELECT p.id, p.corrects_visit_id, up.depth + 1
             FROM up JOIN app.visit p ON p.tenant_id = $1 AND p.id = up.corrects
             WHERE up.depth < $3
           ), down (id, depth) AS (
             SELECT v.id, 1 FROM app.visit v WHERE v.tenant_id = $1 AND v.id = $2
             UNION ALL
             SELECT n.id, down.depth + 1
             FROM down JOIN app.visit n ON n.tenant_id = $1 AND n.corrects_visit_id = down.id
             WHERE down.depth < $3
           )
           SELECT v.id, v.corrects_visit_id, v.status, v.created_by, v.created_at,
                  EXISTS (SELECT 1 FROM app.visit n
                          WHERE n.tenant_id = v.tenant_id AND n.corrects_visit_id = v.id) AS "replaced!"
           FROM app.visit v
           WHERE v.tenant_id = $1 AND v.id IN (SELECT up.id FROM up UNION SELECT down.id FROM down)"#,
        tenant,
        id,
        MAX_CHAIN
    )
    .fetch_all(tx.conn())
    .await?;
    let ids: Vec<Uuid> = chain.iter().map(|v| v.id).collect();
    let first = chain
        .iter()
        .find(|v| v.corrects_visit_id.is_none_or(|c| !ids.contains(&c)))
        .ok_or(AppError::NotFound)?;
    let this = chain
        .iter()
        .find(|v| v.id == id)
        .ok_or(AppError::NotFound)?;
    let state = if this.replaced {
        RecordState::Replaced
    } else if this.status == "voided" {
        RecordState::Voided
    } else {
        RecordState::Active
    };
    let texts: Vec<String> = ids.iter().map(Uuid::to_string).collect();
    // Members: the chain's visit rows, its one-off sales (no `package` line: the revenue book's
    // predicate, 0021), and the expired-package overrides made for these visits (through the
    // packages the visits used, so the entity index answers). Extra: the package sales of the
    // checkouts (their `sale.create` and `customer_package.create`), which fold into the visit's
    // line ("kèm bán gói X") and are never lines here.
    let rows = sqlx::query!(
        r#"WITH one_off AS (
             SELECT s.id FROM app.sale s
             WHERE s.tenant_id = $1 AND s.visit_id = ANY($2)
               AND NOT EXISTS (SELECT 1 FROM app.sale_line sl
                               WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
                                 AND sl.line_type = 'package')
           ), package_sale AS (
             SELECT s.id FROM app.sale s
             WHERE s.tenant_id = $1 AND s.visit_id = ANY($2)
               AND EXISTS (SELECT 1 FROM app.sale_line sl
                           WHERE sl.tenant_id = s.tenant_id AND sl.sale_id = s.id
                             AND sl.line_type = 'package')
           ), used AS (
             SELECT DISTINCT vl.customer_package_id AS id FROM app.visit_line vl
             WHERE vl.tenant_id = $1 AND vl.visit_id = ANY($2) AND vl.customer_package_id IS NOT NULL
           )
           SELECT a.id AS "id!", true AS "member!" FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'visit' AND a.entity_id = ANY($2)
           UNION ALL
           SELECT a.id, true FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'sale' AND a.entity_id IN (SELECT id FROM one_off)
           UNION ALL
           SELECT a.id, true FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'customer_package'
             AND a.entity_id IN (SELECT id FROM used)
             AND a.action = 'customer_package.use_expired_override'
             AND a.after ->> 'visit_id' = ANY($3)
           UNION ALL
           SELECT a.id, false FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'sale' AND a.action = 'sale.create'
             AND a.entity_id IN (SELECT id FROM package_sale)
           UNION ALL
           SELECT a.id, false FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'customer_package'
             AND a.action = 'customer_package.create'
             AND a.entity_id IN (SELECT cp.id FROM app.customer_package cp
                                 WHERE cp.tenant_id = $1 AND cp.sale_id IN (SELECT id FROM package_sale))"#,
        tenant,
        &ids,
        &texts
    )
    .fetch_all(tx.conn())
    .await?;
    let (members, extra): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.member);
    Ok(Selected {
        kind: RecordKind::Visit,
        members: members.into_iter().map(|r| r.id).collect(),
        extra: extra.into_iter().map(|r| r.id).collect(),
        creation: Creation::Visit(first.id),
        fallback: Some((Some(first.created_by), first.created_at)),
        paper_day: None,
        state,
    })
}

/// A package: its own rows, its sale's (bought, voided, its note, paid from the paper book), and
/// the paper-book import that named it.
async fn package(tx: &mut TenantTx, id: Uuid) -> Result<Selected, AppError> {
    let tenant = tx.tenant().as_uuid();
    let p = sqlx::query!(
        r#"SELECT customer_id, sale_id, origin, starts_on, created_by, created_at,
                  cancelled_at IS NOT NULL AS "cancelled!"
           FROM app.customer_package WHERE tenant_id = $1 AND id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let members = sqlx::query_scalar!(
        r#"SELECT a.id AS "id!" FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'customer_package' AND a.entity_id = $2
           UNION ALL
           SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'sale' AND a.entity_id = $3
             AND a.action IN ('sale.create', 'sale.void', 'sale.note_update', 'sale.paper_book')
           UNION ALL
           SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'customer' AND a.entity_id = $4
             AND a.action = 'opening_balance.import'
             AND a.after -> 'customer_package_ids' ? $5"#,
        tenant,
        id,
        p.sale_id,
        p.customer_id,
        id.to_string()
    )
    .fetch_all(tx.conn())
    .await?;
    let paper = p.origin == "opening_balance";
    Ok(Selected {
        kind: RecordKind::CustomerPackage,
        members: members.into_iter().collect(),
        extra: Vec::new(),
        creation: Creation::Package(id),
        fallback: Some((Some(p.created_by), p.created_at)),
        paper_day: paper.then_some(p.starts_on),
        state: if p.cancelled {
            RecordState::Cancelled
        } else {
            RecordState::Active
        },
    })
}

/// A customer's profile: created, edited, health note changed, or created by a paper-book import.
async fn customer(tx: &mut TenantTx, id: Uuid) -> Result<Selected, AppError> {
    let tenant = tx.tenant().as_uuid();
    let members = sqlx::query_scalar!(
        r#"SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'customer' AND a.entity_id = $2
             AND (a.action IN ('customer.create', 'customer.update', 'customer.health_note_set')
                  OR (a.action = 'opening_balance.import'
                      AND a.after ->> 'customer_created' = 'true'))"#,
        tenant,
        id
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(Selected {
        kind: RecordKind::Customer,
        members: members.into_iter().collect(),
        extra: Vec::new(),
        creation: Creation::Customer,
        fallback: None,
        paper_day: None,
        state: RecordState::Active,
    })
}

/// "Tính là sửa" (spec D9, D10, architecture §4.4), on a line as the caller sees it: a visit
/// whose time or technician changed, a correction, a conversion to a package (one line even
/// though it writes two rows); a customer profile edited. A package's life is never an edit.
pub fn counts_as_edit(kind: RecordKind, line: &ActivityEntry) -> bool {
    let action = Action::parse(&line.action);
    match kind {
        RecordKind::Visit => match action {
            Some(Action::VisitCorrect | Action::VisitConvertToPackage) => true,
            Some(Action::VisitUpdate) => line
                .changes
                .iter()
                .any(|c| c.field == "check_in_at" || c.field == "staff"),
            _ => false,
        },
        RecordKind::Customer => action == Some(Action::CustomerUpdate),
        RecordKind::CustomerPackage => false,
    }
}

/// Only what the caller may read on the original screens (spec D7, D8). `None` drops the line.
pub fn entitled(
    mut line: ActivityEntry,
    money: bool,
    notes: bool,
    phones: bool,
) -> Option<ActivityEntry> {
    let action = Action::parse(&line.action);
    if !money {
        line.amount = None;
        line.discount = None;
    }
    if !notes {
        match action {
            Some(Action::CustomerHealthNoteSet | Action::SaleNoteUpdate) => return None,
            Some(Action::VisitUpdate) => {
                let had = !line.changes.is_empty();
                line.changes.retain(|c| c.field != "session_note");
                if had && line.changes.is_empty() {
                    return None;
                }
            }
            _ => {}
        }
    }
    if !phones && action == Some(Action::CustomerUpdate) {
        for change in line.changes.iter_mut().filter(|c| c.field == "phone") {
            change.old = None;
            change.new = None;
        }
    }
    Some(line)
}

/// Who a record's own `created_by` names (a spa login), with the person filter's "left" rule.
async fn person(tx: &mut TenantTx, user: Uuid) -> Result<ActivityActor, AppError> {
    let tenant = tx.tenant().as_uuid();
    let row = sqlx::query!(
        r#"SELECT u.full_name AS "name!",
                  (EXISTS (SELECT 1 FROM app.membership m
                           WHERE m.tenant_id = $1 AND m.user_id = u.id AND m.status <> 'active')
                   OR EXISTS (SELECT 1 FROM app.staff st
                              WHERE st.tenant_id = $1 AND st.user_id = u.id
                                AND NOT EXISTS (
                                  SELECT 1 FROM app.staff_employment e
                                  WHERE e.tenant_id = st.tenant_id AND e.staff_id = st.id
                                    AND e.ended_on IS NULL))) AS "left!"
           FROM app.user_account u WHERE u.id = $2"#,
        tenant,
        user
    )
    .fetch_optional(tx.conn())
    .await?;
    Ok(ActivityActor {
        kind: ActorKind::User,
        user_id: Some(user),
        name: row.as_ref().map(|r| r.name.clone()),
        left: row.and_then(|r| r.left.then_some(true)),
    })
}

/// `GET /record-history/{kind}/{id}`.
pub async fn read(
    pool: &PgPool,
    actor: &Actor,
    route: RecordRoute,
    id: Uuid,
) -> Result<RecordHistory, AppError> {
    require_route(actor, route)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let anchor = resolve(&mut tx, route, id).await?;
    require_anchor(actor, anchor)?;
    let (selected, record) = match anchor {
        Anchor::Visit(v) => (visit(&mut tx, v).await?, RecordRef { kind: RecordKind::Visit, id: v }),
        Anchor::Package(p) => (
            package(&mut tx, p).await?,
            RecordRef {
                kind: RecordKind::CustomerPackage,
                id: p,
            },
        ),
        Anchor::Customer(c) => (
            customer(&mut tx, c).await?,
            RecordRef {
                kind: RecordKind::Customer,
                id: c,
            },
        ),
    };
    let profile = kernel::tenant::profile(&mut tx).await?;
    let tz = profile.timezone;
    let ids: Vec<Uuid> = selected
        .members
        .iter()
        .chain(&selected.extra)
        .copied()
        .collect();
    let rows = activity::rows_by_ids(&mut tx, tz.name(), &ids).await?;
    let fallback = match selected.fallback {
        Some((Some(user), at)) => Some((person(&mut tx, user).await?, at)),
        _ => None,
    };
    tx.commit().await?;

    // A paper-book import also wrote the `customer_package.create` of each package it made and,
    // when it made the customer, the `customer.create`: the import is the one line saying so
    // ("Nhập từ sổ giấy") and the creation (D12). Only imports that made this record are selected.
    let imported = rows
        .iter()
        .any(|r| Action::parse(&r.action) == Some(Action::OpeningBalanceImport));
    let rows: Vec<Row> = rows
        .into_iter()
        .filter(|r| {
            let written_by_import = match Action::parse(&r.action) {
                Some(Action::CustomerPackageCreate) => {
                    r.after
                        .as_ref()
                        .and_then(|a| a.get("origin"))
                        .and_then(serde_json::Value::as_str)
                        == Some("opening_balance")
                }
                Some(Action::CustomerCreate) => true,
                _ => false,
            };
            !(imported && written_by_import)
        })
        .collect();
    let creation_row = rows
        .iter()
        .filter(|r| selected.creation.is(r))
        .min_by_key(|r| (r.at, r.id))
        .map(|r| r.id);
    let lines = fold(rows, &selected.members);
    let line_of = |row: Uuid| {
        lines
            .iter()
            .position(|(root, folded, _)| *root == row || folded.contains(&row))
    };
    let created_line = creation_row.and_then(line_of);
    let created_id = created_line.map(|i| lines[i].0);
    let money = actor.can(Permission::SaleRead);
    let notes = actor.can(Permission::CustomerHealthNotesRead);
    let phones = actor.can(Permission::AuditRead);
    let entries: Vec<ActivityEntry> = lines
        .into_iter()
        .filter_map(|(_, _, entry)| entitled(entry, money, notes, phones))
        .collect();

    let creating = entries.iter().find(|e| Some(e.id) == created_id);
    let created = match (selected.paper_day, creating) {
        (Some(day), line) => Some(RecordCreated {
            actor: line.map(|e| e.actor.clone()),
            at: None,
            day,
            source: RecordSource::PaperBook,
        }),
        (None, Some(e)) => Some(RecordCreated {
            actor: Some(e.actor.clone()),
            at: (Action::parse(&e.action) != Some(Action::OpeningBalanceImport)).then_some(e.at),
            day: e.day,
            source: if Action::parse(&e.action) == Some(Action::OpeningBalanceImport) {
                RecordSource::PaperBook
            } else {
                RecordSource::Audit
            },
        }),
        (None, None) => fallback.map(|(who, at)| RecordCreated {
            actor: Some(who),
            at: Some(at),
            day: at.with_timezone(&tz).date_naive(),
            source: RecordSource::Record,
        }),
    };
    let last = entries
        .last()
        .filter(|e| Some(e.id) != created_id)
        .map(|e| RecordLast {
            actor: e.actor.clone(),
            at: e.at,
            day: e.day,
        });
    let edits = entries
        .iter()
        .filter(|e| counts_as_edit(selected.kind, e))
        .count();
    let omitted = entries.len().saturating_sub(MAX_LINES);
    let items: Vec<ActivityEntry> = entries.into_iter().skip(omitted).collect();
    Ok(RecordHistory {
        record,
        summary: RecordSummary {
            created,
            last,
            edits: u32::try_from(edits).unwrap_or(u32::MAX),
            state: selected.state,
        },
        items,
        omitted: u32::try_from(omitted).unwrap_or(u32::MAX),
    })
}

/// One line per action inside the selected rows (spec D6, architecture §4.3 step 5): a row is a
/// line when it is a member and folds into no other selected row; the others go into the line they
/// fold into, and an extra row that folds into none is dropped. Oldest first, each with the ids of
/// its root and folded rows.
fn fold(rows: Vec<Row>, members: &HashSet<Uuid>) -> Vec<(Uuid, Vec<Uuid>, ActivityEntry)> {
    let covered: HashSet<Uuid> = rows
        .iter()
        .filter(|r| rows.iter().any(|o| o.id != r.id && r.folds_into(o)))
        .map(|r| r.id)
        .collect();
    let rows: Vec<Row> = rows
        .into_iter()
        .filter(|r| members.contains(&r.id) || covered.contains(&r.id))
        .map(|mut r| {
            r.root = !covered.contains(&r.id);
            r
        })
        .collect();
    let mut lines: Vec<(Uuid, Vec<Uuid>, ActivityEntry)> = activity::fold(rows, usize::MAX)
        .into_iter()
        .map(|(root, folded)| {
            let id = root.id;
            let inner = folded.iter().map(|r| r.id).collect();
            (id, inner, activity::line(root, &folded))
        })
        .collect();
    lines.sort_by_key(|(_, _, e)| (e.at, e.id));
    lines
}

/// The visits among `ids` that read "Đã sửa" (`edited`, architecture §4.4): replaced or a
/// replacement ("Sửa lượt"), or a `visit.update` that moved the time or changed a technician of
/// a line. Exactly the visits whose history counts at least one edit ([`counts_as_edit`]). The
/// action list repeats the partial index's (`audit_log_edit_idx`, 0052) so its predicate holds
/// for a prepared statement.
pub async fn edited_visits(tx: &mut TenantTx, ids: &[Uuid]) -> Result<HashSet<Uuid>, AppError> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let tenant = tx.tenant().as_uuid();
    let rows = sqlx::query_scalar!(
        r#"SELECT v.id FROM app.visit v
           WHERE v.tenant_id = $1 AND v.id = ANY($2)
             AND (v.corrects_visit_id IS NOT NULL
                  OR EXISTS (SELECT 1 FROM app.visit w
                             WHERE w.tenant_id = v.tenant_id AND w.corrects_visit_id = v.id)
                  OR EXISTS (
                    SELECT 1 FROM app.audit_log a
                    WHERE a.tenant_id = v.tenant_id AND a.entity_type = 'visit' AND a.entity_id = v.id
                      AND a.action IN ('visit.update', 'appointment.update')
                      AND a.action = 'visit.update'
                      AND (a.after ->> 'check_in_at' IS NOT NULL
                           OR EXISTS (
                             SELECT 1 FROM jsonb_array_elements(
                               CASE WHEN jsonb_typeof(a.after -> 'staff') = 'array'
                                    THEN a.after -> 'staff' ELSE '[]'::jsonb END) e
                             WHERE e ->> 'from' IS DISTINCT FROM e ->> 'to'))))"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity_dto::{ActivityChange, ActivityKind, ChangeFormat};
    use kernel::{TenantId, UserId};

    fn entry(action: Action, changes: &[(&str, Option<&str>, Option<&str>)]) -> ActivityEntry {
        ActivityEntry {
            id: Uuid::now_v7(),
            at: DateTime::<Utc>::UNIX_EPOCH,
            day: NaiveDate::default(),
            actor: ActivityActor {
                kind: ActorKind::User,
                user_id: None,
                name: None,
                left: None,
            },
            kind: ActivityKind::FrontDesk,
            action: action.as_str().to_owned(),
            subject: None,
            changes: changes
                .iter()
                .map(|(field, old, new)| ActivityChange {
                    field: (*field).to_owned(),
                    format: ChangeFormat::Text,
                    old: old.map(str::to_owned),
                    new: new.map(str::to_owned),
                })
                .collect(),
            amount: Some(300_000),
            discount: Some(10_000),
            detail: None,
            reason: None,
            packages: Vec::new(),
            folded: Vec::new(),
        }
    }

    #[test]
    fn what_counts_as_an_edit() {
        use RecordKind as K;
        let staff = entry(Action::VisitUpdate, &[("staff", Some("Thu"), Some("Hoa"))]);
        let time = entry(Action::VisitUpdate, &[("check_in_at", None, Some("x"))]);
        let only_note = entry(Action::VisitUpdate, &[("session_note", None, Some("true"))]);
        let only_companions = entry(Action::VisitUpdate, &[("companions", None, Some("1"))]);
        let nothing = entry(Action::VisitUpdate, &[]);
        assert!(counts_as_edit(K::Visit, &staff));
        assert!(counts_as_edit(K::Visit, &time));
        assert!(!counts_as_edit(K::Visit, &only_note));
        assert!(!counts_as_edit(K::Visit, &only_companions));
        assert!(!counts_as_edit(K::Visit, &nothing));
        assert!(counts_as_edit(K::Visit, &entry(Action::VisitCorrect, &[])));
        assert!(counts_as_edit(K::Visit, &entry(Action::VisitConvertToPackage, &[])));
        // Life, not edits (D9): an upsell, a void, a payment.
        for a in [Action::VisitAddition, Action::VisitVoid, Action::VisitCreate, Action::SaleCreate] {
            assert!(!counts_as_edit(K::Visit, &entry(a, &[])), "{a:?}");
        }
        assert!(counts_as_edit(K::Customer, &entry(Action::CustomerUpdate, &[])));
        assert!(!counts_as_edit(K::Customer, &entry(Action::CustomerHealthNoteSet, &[])));
        for a in [
            Action::CustomerPackageExtend,
            Action::CustomerPackageCancel,
            Action::CustomerPackageUseExpiredOverride,
            Action::SaleNoteUpdate,
        ] {
            assert!(!counts_as_edit(K::CustomerPackage, &entry(a, &[])), "{a:?}");
        }
    }

    #[test]
    fn lines_keep_only_what_the_caller_may_read() {
        // No money without sale.read.
        let e = entitled(entry(Action::VisitCreate, &[]), false, true, true);
        assert_eq!(e.map(|e| (e.amount, e.discount)), Some((None, None)));
        // A note-only edit disappears without health notes; with another change it stays.
        let note = entry(Action::VisitUpdate, &[("session_note", None, Some("true"))]);
        assert!(entitled(note.clone(), true, false, true).is_none());
        assert!(entitled(note, true, true, true).is_some());
        let both = entry(
            Action::VisitUpdate,
            &[("staff", Some("Thu"), Some("Hoa")), ("session_note", None, Some("false"))],
        );
        let kept = entitled(both, true, false, true).map(|e| e.changes.len());
        assert_eq!(kept, Some(1));
        for a in [Action::CustomerHealthNoteSet, Action::SaleNoteUpdate] {
            assert!(entitled(entry(a, &[]), true, false, true).is_none(), "{a:?}");
            assert!(entitled(entry(a, &[]), true, true, true).is_some(), "{a:?}");
        }
        // The profile note ("Ghi chú hồ sơ") is not a health note.
        let profile_note = entry(Action::CustomerUpdate, &[("note", None, None)]);
        assert!(entitled(profile_note, true, false, false).is_some());
        // A phone change keeps its name and loses both values without audit.read (D8).
        let phone = entry(
            Action::CustomerUpdate,
            &[("phone", Some("+84905111222"), Some("+84912333444")), ("full_name", Some("A"), Some("B"))],
        );
        let seen = entitled(phone.clone(), true, true, false).unwrap_or_else(|| unreachable!());
        assert_eq!(seen.changes[0].field, "phone");
        assert_eq!((seen.changes[0].old.as_deref(), seen.changes[0].new.as_deref()), (None, None));
        assert_eq!(seen.changes[1].new.as_deref(), Some("B"));
        let owner = entitled(phone, true, true, true).unwrap_or_else(|| unreachable!());
        assert_eq!(owner.changes[0].new.as_deref(), Some("+84912333444"));
    }

    #[test]
    fn the_route_and_the_anchor_are_both_required() {
        use Permission as P;
        let actor = |perms: &[P]| {
            Actor::new(
                UserId::from_uuid(Uuid::now_v7()),
                TenantId::from_uuid(Uuid::now_v7()),
                perms.iter().copied(),
                None,
            )
        };
        // Without the switch, nothing (even with every read right).
        let no_switch = actor(&[P::VisitRead, P::SaleRead, P::CustomerRead]);
        for route in [
            RecordRoute::Visit,
            RecordRoute::Sale,
            RecordRoute::Refund,
            RecordRoute::CustomerPackage,
            RecordRoute::Customer,
        ] {
            assert!(require_route(&no_switch, route).is_err(), "{route:?}");
        }
        // B1: sale.read + customer.read without visit.read passes the sale route, then a one-off
        // sale's anchor (the visit) is refused and a package sale's is allowed.
        let cashier = actor(&[P::RecordHistoryRead, P::SaleRead, P::CustomerRead]);
        assert!(require_route(&cashier, RecordRoute::Sale).is_ok());
        assert!(require_anchor(&cashier, Anchor::Visit(Uuid::nil())).is_err());
        assert!(require_anchor(&cashier, Anchor::Package(Uuid::nil())).is_ok());
        // A refund needs both money and customers.
        let money_only = actor(&[P::RecordHistoryRead, P::SaleRead]);
        assert!(require_route(&money_only, RecordRoute::Refund).is_err());
        assert!(require_route(&cashier, RecordRoute::Refund).is_ok());
    }
}
