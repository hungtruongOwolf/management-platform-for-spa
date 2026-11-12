//! "Ai ghi, ai sửa" on each record (ADR-0022, spec record-history, architecture §4): the history of
//! one visit, package, customer, booking, staff-book cell or bonus / penalty, read from
//! `app.audit_log` with the same row reader and the same one-line-per-action folding as the
//! owner's trail ([`crate::activity`]).
//!
//! Authorization, in this order (F17): `record.history.read`, the route's own read permission
//! ([`require_route`], [`require_staff_book`]), then the record is re-anchored (a sale → its visit
//! or its package, a refund → its package; `404` when it is not the spa's, or — a staff record —
//! not in the caller's books: their own, a day of a month the person did not work) and the read
//! permission of the anchor's kind is required ([`require_anchor`], exhaustive), all before any
//! history row is read.
//!
//! Membership is by audit row id (spec D6): each kind selects exactly the rows that are about it,
//! never "the same instant" or "the same entity" in general, so a package's later life never
//! leaks into the visit that sold it, and a save of the book that wrote many days never brings
//! another day into a cell. What a line shows follows the caller's rights on the original screens
//! (D7, D8): money with `sale.read` (a bonus / penalty with the book's own right), note changes
//! with `customer.health_notes.read`, a phone's values with `audit.read`.

use crate::activity::{self, Row};
use crate::activity_dto::{ActivityActor, ActivityEntry, ActorKind};
use crate::history_dto::{
    RecordCreated, RecordHistory, RecordKind, RecordLast, RecordRef, RecordSource, RecordState,
    RecordSummary, StaffBook,
};
use chrono::{DateTime, NaiveDate, Utc};
use hr::month::Month;
use hr::scope::RecordScope;
use kernel::audit::Action;
use kernel::{Actor, AppError, Permission, TenantTx};
use sqlx::PgPool;
use std::collections::HashSet;
use uuid::Uuid;

/// Newest lines kept (older ones are counted in `omitted`).
const MAX_LINES: usize = 100;
/// Longest "Sửa lượt" chain followed each way; also a bonus / penalty's chain of edits.
const MAX_CHAIN: i32 = 20;

/// One line of a history: the row it is, the rows folded into it, and the entry.
type Line = (Uuid, Vec<Uuid>, ActivityEntry);

/// The record a route names by its id, before re-anchoring. A staff-book cell is named by a
/// person and a day ([`read_staff_day`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordRoute {
    Visit,
    Sale,
    Refund,
    CustomerPackage,
    Customer,
    Appointment,
    Adjustment,
}

/// The record whose history is read, after re-anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Visit(Uuid),
    Package(Uuid),
    Customer(Uuid),
    Appointment(Uuid),
    /// A bonus / penalty (the chain of its edits is found when selecting).
    Adjustment(Uuid),
    /// One person's cell of one book on one day.
    StaffDay(Uuid, NaiveDate, StaffBook),
}

/// The right of a staff book: the one its screen and its `PUT` need.
const fn book_right(book: StaffBook) -> Permission {
    match book {
        StaffBook::Attendance => Permission::AttendanceRecord,
        StaffBook::DailyReview => Permission::EvaluationRecord,
    }
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
        RecordRoute::Appointment => actor.require(P::AppointmentManage),
        RecordRoute::Adjustment => actor.require(P::EvaluationRecord),
    }
}

/// The rights of `GET /record-history/staff-day/…`: the switch, then the book's right. The book is
/// a query parameter, so before it is parsed (`None`) the right of either book lets the request
/// on (a caller with neither is `403` whatever they send, K21); once parsed, its own right.
pub fn require_staff_book(actor: &Actor, book: Option<StaffBook>) -> Result<(), AppError> {
    actor.require(Permission::RecordHistoryRead)?;
    match book {
        Some(book) => actor.require(book_right(book)),
        None => actor
            .require(Permission::AttendanceRecord)
            .or_else(|_| actor.require(Permission::EvaluationRecord)),
    }
}

/// The read right of the anchor's kind (B1): a sale of one-off services is its visit's history
/// (`visit.read`), a package sale or a refund its package's (`customer.read`); a booking the
/// calendar's, a staff record its book's (whose scope is checked when selecting, `404`).
fn require_anchor(actor: &Actor, anchor: Anchor) -> Result<(), AppError> {
    match anchor {
        Anchor::Visit(_) => actor.require(Permission::VisitRead),
        Anchor::Package(_) | Anchor::Customer(_) => actor.require(Permission::CustomerRead),
        Anchor::Appointment(_) => actor.require(Permission::AppointmentManage),
        Anchor::Adjustment(_) => actor.require(Permission::EvaluationRecord),
        Anchor::StaffDay(_, _, book) => actor.require(book_right(book)),
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
        RecordRoute::Appointment => sqlx::query_scalar!(
            "SELECT id FROM app.appointment WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Appointment),
        RecordRoute::Adjustment => sqlx::query_scalar!(
            "SELECT id FROM app.staff_adjustment WHERE tenant_id = $1 AND id = $2",
            tenant,
            id
        )
        .fetch_optional(tx.conn())
        .await?
        .map(Anchor::Adjustment),
    };
    anchor.ok_or(AppError::NotFound)
}

/// What the history of one anchor is made of.
struct Selected {
    kind: RecordKind,
    /// The record as the reply names it (absent for a staff-book cell).
    id: Option<Uuid>,
    /// Rows of the history (spec D6).
    members: HashSet<Uuid>,
    /// Rows that may only fold into a member line (a package sold in a visit's checkout), never a
    /// line of their own.
    extra: Vec<Uuid>,
    /// The edits of a bonus / penalty: (the entry replaced, the entry that replaced it), each read
    /// as one line ([`activity::replaced_adjustment`]).
    edits: Vec<(Uuid, Uuid)>,
    /// The creating action's row, by its action and entity (D12).
    creation: Creation,
    /// Without a creating row: who / when from the record itself.
    fallback: Option<(Option<Uuid>, DateTime<Utc>)>,
    /// A paper-book package: its book's day.
    paper_day: Option<NaiveDate>,
    state: RecordState,
}

impl Selected {
    /// What a line's money is shown under (D7): a bonus / penalty is the staff book's own figure.
    const fn money_right(&self) -> Permission {
        match self.kind {
            RecordKind::Adjustment => Permission::EvaluationRecord,
            _ => Permission::SaleRead,
        }
    }
}

/// Which row is the creation.
enum Creation {
    /// `visit.create` of the chain's first visit.
    Visit(Uuid),
    /// `customer_package.create` of the package or the import that named it.
    Package(Uuid),
    /// `customer.create`, or an import that created the customer.
    Customer,
    /// `appointment.create` of the booking.
    Appointment(Uuid),
    /// `staff.adjustment` of the chain's first entry.
    Adjustment(Uuid),
    /// A staff-book cell has no creating action: its first row is.
    First,
}

impl Creation {
    fn is(&self, row: &Row) -> bool {
        match (self, Action::parse(&row.action)) {
            (Creation::Visit(id), Some(Action::VisitCreate))
            | (Creation::Package(id), Some(Action::CustomerPackageCreate))
            | (Creation::Appointment(id), Some(Action::AppointmentCreate)) => row.entity_id == *id,
            (Creation::Adjustment(id), Some(Action::StaffAdjustment)) => {
                row.adjustment_id() == Some(*id)
            }
            (Creation::Package(_), Some(Action::OpeningBalanceImport))
            | (Creation::Customer, Some(Action::CustomerCreate | Action::OpeningBalanceImport))
            | (Creation::First, _) => true,
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
        id: Some(id),
        members: members.into_iter().map(|r| r.id).collect(),
        extra: extra.into_iter().map(|r| r.id).collect(),
        edits: Vec::new(),
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
        id: Some(id),
        members: members.into_iter().collect(),
        extra: Vec::new(),
        edits: Vec::new(),
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
        id: Some(id),
        members: members.into_iter().collect(),
        extra: Vec::new(),
        edits: Vec::new(),
        creation: Creation::Customer,
        fallback: None,
        paper_day: None,
        state: RecordState::Active,
    })
}

/// A booking: every row written on it (made, moved, arrived, done, missed, cancelled).
async fn appointment(tx: &mut TenantTx, id: Uuid) -> Result<Selected, AppError> {
    let tenant = tx.tenant().as_uuid();
    let a = sqlx::query!(
        "SELECT status, created_by, created_at FROM app.appointment WHERE tenant_id = $1 AND id = $2",
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let members = sqlx::query_scalar!(
        r#"SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'appointment' AND a.entity_id = $2"#,
        tenant,
        id
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(Selected {
        kind: RecordKind::Appointment,
        id: Some(id),
        members: members.into_iter().collect(),
        extra: Vec::new(),
        edits: Vec::new(),
        creation: Creation::Appointment(id),
        fallback: Some((Some(a.created_by), a.created_at)),
        paper_day: None,
        state: if a.status == "cancelled" {
            RecordState::Cancelled
        } else {
            RecordState::Active
        },
    })
}

/// One cell of a staff book (spec edge case 13): in the caller's books (never their own profile,
/// [`RecordScope::lists`]) and in a month the person worked (`employed_in`, as the sheets list
/// them), else `404`. Its rows are the book's rows on that person whose own `date` is the day:
/// a save of the book writes many days at one instant, and only this day's belong here. The
/// literal action list repeats the partial index's (`audit_log_staff_day_idx`, 0052) and the
/// date is compared through the same expression, so the index answers.
async fn staff_day(
    tx: &mut TenantTx,
    actor: &Actor,
    staff: Uuid,
    date: NaiveDate,
    book: StaffBook,
) -> Result<Selected, AppError> {
    let profile = org::profile::profile_in(tx, staff).await?;
    let scope = RecordScope::load(tx, actor).await?;
    if !scope.lists(staff) || !hr::period::employed_in(&profile, Month::of(date)) {
        return Err(AppError::NotFound);
    }
    let (action, kind) = match book {
        StaffBook::Attendance => (Action::AttendanceRecord, RecordKind::Attendance),
        StaffBook::DailyReview => (Action::EvaluationDailyReview, RecordKind::DailyReview),
    };
    let tenant = tx.tenant().as_uuid();
    let members = sqlx::query_scalar!(
        r#"SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_id = $2
             AND a.action IN ('attendance.record', 'evaluation.daily_review')
             AND coalesce(a.after ->> 'date', a.before ->> 'date') = $3::text
             AND a.action = $4 AND a.entity_type = 'staff'"#,
        tenant,
        staff,
        date.to_string(),
        action.as_str()
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(Selected {
        kind,
        id: None,
        members: members.into_iter().collect(),
        extra: Vec::new(),
        edits: Vec::new(),
        creation: Creation::First,
        fallback: None,
        paper_day: None,
        state: RecordState::Active,
    })
}

/// A bonus / penalty as it stands after its edits: `PUT /adjustments/{id}` voids the entry and
/// records the corrected one in one transaction, maybe for another person (S13), so an edit is a
/// `staff.adjustment_void` and a `staff.adjustment` of the same instant by the same person — and
/// only those two (architecture §4.3: a batch that wrote several is no edit). The chain is
/// followed both ways from `id` (≤ [`MAX_CHAIN`] each), every person in it must be in the caller's
/// books (else `404`), and its rows are the two actions' rows about its entries.
async fn adjustment(tx: &mut TenantTx, actor: &Actor, id: Uuid) -> Result<Selected, AppError> {
    let tenant = tx.tenant().as_uuid();
    let this = sqlx::query!(
        r#"SELECT staff_id, voided_at IS NOT NULL AS "voided!", created_by, created_at
           FROM app.staff_adjustment WHERE tenant_id = $1 AND id = $2"#,
        tenant,
        id
    )
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let mut chain = vec![Entry {
        id,
        staff: this.staff_id,
        created_by: this.created_by,
        created_at: this.created_at,
    }];
    for _ in 0..MAX_CHAIN {
        let Some(head) = chain.first() else { break };
        match edit_link(tx, head, Link::Replaced).await? {
            Some(before) => chain.insert(0, before),
            None => break,
        }
    }
    for _ in 0..MAX_CHAIN {
        let Some(tail) = chain.last() else { break };
        match edit_link(tx, tail, Link::Replacement).await? {
            Some(after) => chain.push(after),
            None => break,
        }
    }
    let scope = RecordScope::load(tx, actor).await?;
    if chain.iter().any(|e| !scope.lists(e.staff)) {
        return Err(AppError::NotFound);
    }
    let staff: Vec<Uuid> = chain.iter().map(|e| e.staff).collect();
    let ids: Vec<String> = chain.iter().map(|e| e.id.to_string()).collect();
    let members = sqlx::query_scalar!(
        r#"SELECT a.id FROM app.audit_log a
           WHERE a.tenant_id = $1 AND a.entity_type = 'staff' AND a.entity_id = ANY($2)
             AND a.action IN ('staff.adjustment', 'staff.adjustment_void')
             AND coalesce(a.after ->> 'adjustment_id', a.before ->> 'adjustment_id') = ANY($3)"#,
        tenant,
        &staff,
        &ids
    )
    .fetch_all(tx.conn())
    .await?;
    // The chain grew both ways from `id`: an entry after it exists exactly when it was edited.
    let replaced = chain.last().is_some_and(|e| e.id != id);
    let (first_id, fallback) = chain
        .first()
        .map_or((id, (Some(this.created_by), this.created_at)), |e| {
            (e.id, (Some(e.created_by), e.created_at))
        });
    Ok(Selected {
        kind: RecordKind::Adjustment,
        id: Some(id),
        members: members.into_iter().collect(),
        extra: Vec::new(),
        edits: chain.windows(2).map(|w| (w[0].id, w[1].id)).collect(),
        creation: Creation::Adjustment(first_id),
        fallback: Some(fallback),
        paper_day: None,
        state: if replaced {
            RecordState::Replaced
        } else if this.voided {
            RecordState::Voided
        } else {
            RecordState::Active
        },
    })
}

/// One bonus / penalty of a chain of edits.
struct Entry {
    id: Uuid,
    staff: Uuid,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

/// Which way [`edit_link`] looks from an entry.
#[derive(Clone, Copy)]
enum Link {
    /// The entry this one replaced (it was recorded by an edit).
    Replaced,
    /// The entry that replaced this one (it was voided by an edit).
    Replacement,
}

/// The neighbour of `entry` in its chain of edits, if any: from the entry's own row (its
/// `staff.adjustment` looking back, its `staff.adjustment_void` looking forward), the rows of the
/// same instant by the same person must be exactly one of each, and the other one names the
/// neighbour (which may be another person's, S13).
async fn edit_link(tx: &mut TenantTx, entry: &Entry, way: Link) -> Result<Option<Entry>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let (own, other) = match way {
        Link::Replaced => (Action::StaffAdjustment, Action::StaffAdjustmentVoid),
        Link::Replacement => (Action::StaffAdjustmentVoid, Action::StaffAdjustment),
    };
    let found = sqlx::query!(
        r#"WITH own AS (
             SELECT a.at, a.actor_kind, a.actor_user_id FROM app.audit_log a
             WHERE a.tenant_id = $1 AND a.entity_type = 'staff' AND a.entity_id = $2
               AND a.action = $4
               AND coalesce(a.after ->> 'adjustment_id', a.before ->> 'adjustment_id') = $3
             ORDER BY a.at, a.id LIMIT 1
           ), moment AS (
             SELECT a.action,
                    coalesce(a.after ->> 'adjustment_id', a.before ->> 'adjustment_id') AS adjustment
             FROM app.audit_log a JOIN own o ON a.at = o.at AND a.actor_kind = o.actor_kind
               AND a.actor_user_id IS NOT DISTINCT FROM o.actor_user_id
             WHERE a.tenant_id = $1 AND a.action IN ('staff.adjustment', 'staff.adjustment_void')
           )
           SELECT x.id, x.staff_id, x.created_by, x.created_at FROM app.staff_adjustment x
           WHERE x.tenant_id = $1
             AND (SELECT count(*) FROM moment m WHERE m.action = 'staff.adjustment') = 1
             AND (SELECT count(*) FROM moment m WHERE m.action = 'staff.adjustment_void') = 1
             AND x.id = (SELECT m.adjustment::uuid FROM moment m WHERE m.action = $5)"#,
        tenant,
        entry.staff,
        entry.id.to_string(),
        own.as_str(),
        other.as_str()
    )
    .fetch_optional(tx.conn())
    .await?;
    Ok(found.map(|x| Entry {
        id: x.id,
        staff: x.staff_id,
        created_by: x.created_by,
        created_at: x.created_at,
    }))
}

/// "Tính là sửa" (spec D9, D10, architecture §4.4), on a line as the caller sees it (`first`: the
/// oldest line of the history): a visit whose time or technician changed, a correction, a
/// conversion to a package (one line even though it writes two rows); a customer profile edited;
/// a booking moved or given another technician; a staff-book cell written again (every line after
/// the first); a bonus / penalty edited (its void and new entry are one line). A package's life,
/// a booking's arrival, completion or cancellation are never edits.
pub fn counts_as_edit(kind: RecordKind, line: &ActivityEntry, first: bool) -> bool {
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
        RecordKind::Appointment => {
            action == Some(Action::AppointmentUpdate)
                && line
                    .changes
                    .iter()
                    .any(|c| c.field == "starts_at" || c.field == "staff")
        }
        RecordKind::Attendance | RecordKind::DailyReview => !first,
        RecordKind::Adjustment => {
            action == Some(Action::StaffAdjustment)
                && line
                    .folded
                    .iter()
                    .any(|f| f == Action::StaffAdjustmentVoid.as_str())
        }
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
    history(tx, actor, anchor).await
}

/// `GET /record-history/staff-day/{staff_id}/{date}?book=`: one person's cell of one book.
pub async fn read_staff_day(
    pool: &PgPool,
    actor: &Actor,
    staff: Uuid,
    date: NaiveDate,
    book: StaffBook,
) -> Result<RecordHistory, AppError> {
    require_staff_book(actor, Some(book))?;
    let tx = TenantTx::for_actor(pool, actor).await?;
    history(tx, actor, Anchor::StaffDay(staff, date, book)).await
}

/// The history of a resolved anchor: its kind's read right, its rows, its lines as the caller may
/// see them, and the summary.
async fn history(
    mut tx: TenantTx,
    actor: &Actor,
    anchor: Anchor,
) -> Result<RecordHistory, AppError> {
    require_anchor(actor, anchor)?;
    let selected = match anchor {
        Anchor::Visit(v) => visit(&mut tx, v).await?,
        Anchor::Package(p) => package(&mut tx, p).await?,
        Anchor::Customer(c) => customer(&mut tx, c).await?,
        Anchor::Appointment(a) => appointment(&mut tx, a).await?,
        Anchor::Adjustment(a) => adjustment(&mut tx, actor, a).await?,
        Anchor::StaffDay(staff, date, book) => staff_day(&mut tx, actor, staff, date, book).await?,
    };
    let record = RecordRef {
        kind: selected.kind,
        id: selected.id,
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
    let (rows, mut edited) = edits_of_adjustment(rows, &selected.edits);
    let mut lines = fold(rows, &selected.members);
    if !edited.is_empty() {
        lines.append(&mut edited);
        lines.sort_by_key(|(_, _, e)| (e.at, e.id));
    }
    let line_of = |row: Uuid| {
        lines
            .iter()
            .position(|(root, folded, _)| *root == row || folded.contains(&row))
    };
    let created_line = creation_row.and_then(line_of);
    let created_id = created_line.map(|i| lines[i].0);
    let money = actor.can(selected.money_right());
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
        .enumerate()
        .filter(|(i, e)| counts_as_edit(selected.kind, e, *i == 0))
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

/// The edits of a bonus / penalty as one line each (`edits`: replaced entry → its replacement):
/// the void of the replaced entry and the `staff.adjustment` of the new one leave `rows` and come
/// back as one "sửa" line with what changed. A pair missing a row stays as two lines.
fn edits_of_adjustment(mut rows: Vec<Row>, edits: &[(Uuid, Uuid)]) -> (Vec<Row>, Vec<Line>) {
    let mut lines = Vec::with_capacity(edits.len());
    for (was, now) in edits {
        let find = |rows: &[Row], action: Action, about: Uuid| {
            rows.iter()
                .position(|r| r.action == action.as_str() && r.adjustment_id() == Some(about))
        };
        let (Some(v), Some(n)) = (
            find(&rows, Action::StaffAdjustmentVoid, *was),
            find(&rows, Action::StaffAdjustment, *now),
        ) else {
            continue;
        };
        // Remove the later index first so the earlier one stays valid.
        let (void, new) = if v > n {
            let void = rows.remove(v);
            (void, rows.remove(n))
        } else {
            let new = rows.remove(n);
            (rows.remove(v), new)
        };
        let old = find(&rows, Action::StaffAdjustment, *was).and_then(|i| rows.get(i));
        let (id, void_id) = (new.id, void.id);
        lines.push((
            id,
            vec![void_id],
            activity::replaced_adjustment(&void, old, new),
        ));
    }
    (rows, lines)
}

/// One line per action inside the selected rows (spec D6, architecture §4.3 step 5): a row is a
/// line when it is a member and folds into no other selected row; the others go into the line they
/// fold into, and an extra row that folds into none is dropped. Oldest first, each with the ids of
/// its root and folded rows.
fn fold(rows: Vec<Row>, members: &HashSet<Uuid>) -> Vec<Line> {
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
    let mut lines: Vec<Line> = activity::fold(rows, usize::MAX)
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
        let edit = |kind: K, e: &ActivityEntry| counts_as_edit(kind, e, false);
        let staff = entry(Action::VisitUpdate, &[("staff", Some("Thu"), Some("Hoa"))]);
        let time = entry(Action::VisitUpdate, &[("check_in_at", None, Some("x"))]);
        let only_note = entry(Action::VisitUpdate, &[("session_note", None, Some("true"))]);
        let only_companions = entry(Action::VisitUpdate, &[("companions", None, Some("1"))]);
        let nothing = entry(Action::VisitUpdate, &[]);
        assert!(edit(K::Visit, &staff));
        assert!(edit(K::Visit, &time));
        assert!(!edit(K::Visit, &only_note));
        assert!(!edit(K::Visit, &only_companions));
        assert!(!edit(K::Visit, &nothing));
        assert!(edit(K::Visit, &entry(Action::VisitCorrect, &[])));
        assert!(edit(K::Visit, &entry(Action::VisitConvertToPackage, &[])));
        // Life, not edits (D9): an upsell, a void, a payment.
        for a in [
            Action::VisitAddition,
            Action::VisitVoid,
            Action::VisitCreate,
            Action::SaleCreate,
        ] {
            assert!(!edit(K::Visit, &entry(a, &[])), "{a:?}");
        }
        assert!(edit(K::Customer, &entry(Action::CustomerUpdate, &[])));
        assert!(!edit(
            K::Customer,
            &entry(Action::CustomerHealthNoteSet, &[])
        ));
        for a in [
            Action::CustomerPackageExtend,
            Action::CustomerPackageCancel,
            Action::CustomerPackageUseExpiredOverride,
            Action::SaleNoteUpdate,
        ] {
            assert!(!edit(K::CustomerPackage, &entry(a, &[])), "{a:?}");
        }
        // A booking: moved or another technician; not its services, note, companions, nor its
        // life (arrival with the serving technician, completion, a miss, a cancellation).
        let moved = entry(
            Action::AppointmentUpdate,
            &[("starts_at", Some("a"), Some("b"))],
        );
        let other_staff = entry(Action::AppointmentUpdate, &[("staff", Some("Thu"), None)]);
        let services = entry(Action::AppointmentUpdate, &[("services", None, None)]);
        let note = entry(Action::AppointmentUpdate, &[("booking_note", None, None)]);
        assert!(edit(K::Appointment, &moved));
        assert!(edit(K::Appointment, &other_staff));
        assert!(!edit(K::Appointment, &services));
        assert!(!edit(K::Appointment, &note));
        let arrived = entry(
            Action::AppointmentArrive,
            &[("staff", Some("Thu"), Some("Hoa"))],
        );
        assert!(!edit(K::Appointment, &arrived));
        for a in [
            Action::AppointmentCreate,
            Action::AppointmentComplete,
            Action::AppointmentNoShow,
            Action::AppointmentCancel,
            Action::AppointmentUndoArrival,
        ] {
            assert!(!edit(K::Appointment, &entry(a, &[])), "{a:?}");
        }
        // A staff-book cell: every line after the first.
        for kind in [K::Attendance, K::DailyReview] {
            let line = entry(Action::AttendanceRecord, &[]);
            assert!(!counts_as_edit(kind, &line, true));
            assert!(counts_as_edit(kind, &line, false));
        }
        // A bonus / penalty: an edit (the void folded into the new entry), never a plain record
        // or a removal.
        let mut replaced = entry(Action::StaffAdjustment, &[]);
        replaced.folded = vec![Action::StaffAdjustmentVoid.as_str().to_owned()];
        assert!(edit(K::Adjustment, &replaced));
        assert!(!edit(K::Adjustment, &entry(Action::StaffAdjustment, &[])));
        assert!(!edit(
            K::Adjustment,
            &entry(Action::StaffAdjustmentVoid, &[])
        ));
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
            &[
                ("staff", Some("Thu"), Some("Hoa")),
                ("session_note", None, Some("false")),
            ],
        );
        let kept = entitled(both, true, false, true).map(|e| e.changes.len());
        assert_eq!(kept, Some(1));
        for a in [Action::CustomerHealthNoteSet, Action::SaleNoteUpdate] {
            assert!(
                entitled(entry(a, &[]), true, false, true).is_none(),
                "{a:?}"
            );
            assert!(entitled(entry(a, &[]), true, true, true).is_some(), "{a:?}");
        }
        // The profile note ("Ghi chú hồ sơ") is not a health note.
        let profile_note = entry(Action::CustomerUpdate, &[("note", None, None)]);
        assert!(entitled(profile_note, true, false, false).is_some());
        // A phone change keeps its name and loses both values without audit.read (D8).
        let phone = entry(
            Action::CustomerUpdate,
            &[
                ("phone", Some("+84905111222"), Some("+84912333444")),
                ("full_name", Some("A"), Some("B")),
            ],
        );
        let seen = entitled(phone.clone(), true, true, false).unwrap_or_else(|| unreachable!());
        assert_eq!(seen.changes[0].field, "phone");
        assert_eq!(
            (
                seen.changes[0].old.as_deref(),
                seen.changes[0].new.as_deref()
            ),
            (None, None)
        );
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
            RecordRoute::Appointment,
            RecordRoute::Adjustment,
        ] {
            assert!(require_route(&no_switch, route).is_err(), "{route:?}");
        }
        // The staff books: the switch, then a book's right (either one before the book is known).
        let books = actor(&[
            P::AttendanceRecord,
            P::EvaluationRecord,
            P::AppointmentManage,
        ]);
        assert!(require_staff_book(&books, None).is_err());
        assert!(require_route(&books, RecordRoute::Appointment).is_err());
        let attendance = actor(&[P::RecordHistoryRead, P::AttendanceRecord]);
        assert!(require_staff_book(&attendance, None).is_ok());
        assert!(require_staff_book(&attendance, Some(StaffBook::Attendance)).is_ok());
        assert!(require_staff_book(&attendance, Some(StaffBook::DailyReview)).is_err());
        assert!(require_route(&attendance, RecordRoute::Adjustment).is_err());
        let nothing = actor(&[P::RecordHistoryRead, P::VisitRead]);
        assert!(require_staff_book(&nothing, None).is_err());
        assert!(require_route(&nothing, RecordRoute::Appointment).is_err());
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
