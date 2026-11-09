//! Re-engagement follow-ups (spec management-views US5b): a manager calls or messages a customer
//! who has not come back and records when, how, what came of it and when to try again.
//!
//! Append-only: a wrong entry is corrected by recording a new one. The recorder is the session's
//! user. Defaults when no next date is given (lead's decision, EM Q6 2026-11-05; a date entered
//! by hand always wins):
//! - `no_answer` → try again [`NO_ANSWER_RETRY_DAYS`] days after the contact;
//! - `will_come` → check in [`WILL_COME_CHECK_DAYS`] days later if they still have not come (a
//!   visit starts a new absence, so the reminder disappears by itself);
//! - `not_interested` → no date; the re-engagement list leaves them out of "due" for
//!   [`NOT_INTERESTED_PAUSE_DAYS`] days, then shows them once more;
//! - `other` → no date.

use crate::repo::{self, FollowUpInsert, FollowUpRow};
use chrono::{Days, NaiveDate};
use kernel::audit::{self, AuditEntry};
use kernel::idempotency::{self, Claim};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// "Không nghe máy" → gọi lại sau 3 ngày, unless the manager picks another date.
pub const NO_ANSWER_RETRY_DAYS: u64 = 3;
/// "Hẹn sẽ đến" → kiểm lại sau 7 ngày nếu vẫn chưa đến.
pub const WILL_COME_CHECK_DAYS: u64 = 7;
/// "Không muốn" → không làm phiền trong 90 ngày (the inactive list applies it).
pub const NOT_INTERESTED_PAUSE_DAYS: u64 = 90;
/// A contact can be recorded late (yesterday's calls typed in today), within this many days.
pub const MAX_BACKDATE_DAYS: u64 = 30;
/// The next follow-up can be planned at most this far after the contact.
pub const MAX_SCHEDULE_DAYS: u64 = 365;
pub const MAX_NOTE_CHARS: usize = 500;
/// `GET /customers/{id}/follow-ups` returns at most this many, newest first.
pub const LIST_LIMIT: i64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Phone,
    Zalo,
    InPerson,
    Other,
}

impl Channel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Zalo => "zalo",
            Self::InPerson => "in_person",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Phone, Self::Zalo, Self::InPerson, Self::Other]
            .into_iter()
            .find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Reached; says they will come.
    WillCome,
    /// Reached; not interested (stop calling).
    NotInterested,
    NoAnswer,
    Other,
}

impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WillCome => "will_come",
            Self::NotInterested => "not_interested",
            Self::NoAnswer => "no_answer",
            Self::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::WillCome,
            Self::NotInterested,
            Self::NoAnswer,
            Self::Other,
        ]
        .into_iter()
        .find(|o| o.as_str() == s)
    }
}

pub struct FollowUpInput {
    /// Default today (spa time zone).
    pub contacted_on: Option<NaiveDate>,
    pub channel: Channel,
    pub outcome: Outcome,
    pub note: Option<String>,
    /// `None` = the rule's suggestion; `Some(None)` = no next try ("Không hẹn"); `Some(Some(d))`
    /// = that day.
    pub next_follow_up_on: Option<Option<NaiveDate>>,
}

/// A recorded follow-up as the API shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub contacted_on: NaiveDate,
    pub channel: Channel,
    pub outcome: Outcome,
    pub note: Option<String>,
    pub next_follow_up_on: Option<NaiveDate>,
    pub by_user_id: Uuid,
    pub by_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TryFrom<FollowUpRow> for FollowUp {
    type Error = AppError;
    fn try_from(r: FollowUpRow) -> Result<Self, AppError> {
        let corrupt = || AppError::Internal(format!("follow-up {} has unknown enum", r.id));
        Ok(Self {
            channel: Channel::parse(&r.channel).ok_or_else(corrupt)?,
            outcome: Outcome::parse(&r.outcome).ok_or_else(corrupt)?,
            id: r.id,
            customer_id: r.customer_id,
            contacted_on: r.contacted_on,
            note: r.note,
            next_follow_up_on: r.next_follow_up_on,
            by_user_id: r.created_by,
            by_name: r.by_name,
            created_at: r.created_at,
        })
    }
}

/// The date the rules suggest for a contact on `contacted_on` with this outcome.
pub fn suggested_next(outcome: Outcome, contacted_on: NaiveDate) -> Option<NaiveDate> {
    match outcome {
        Outcome::NoAnswer => contacted_on.checked_add_days(Days::new(NO_ANSWER_RETRY_DAYS)),
        Outcome::WillCome => contacted_on.checked_add_days(Days::new(WILL_COME_CHECK_DAYS)),
        Outcome::NotInterested | Outcome::Other => None,
    }
}

/// The stored next date: what was asked (a day, or explicitly none) wins; nothing asked → the
/// suggestion.
pub fn next_follow_up(
    outcome: Outcome,
    contacted_on: NaiveDate,
    requested: Option<Option<NaiveDate>>,
) -> Option<NaiveDate> {
    requested.unwrap_or_else(|| suggested_next(outcome, contacted_on))
}

/// Suggestions for a contact made today, per outcome, so the form can pre-fill the date
/// before saving (`GET /customers/{id}/follow-ups`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestions {
    pub today: NaiveDate,
    pub will_come: Option<NaiveDate>,
    pub not_interested: Option<NaiveDate>,
    pub no_answer: Option<NaiveDate>,
    pub other: Option<NaiveDate>,
}

pub fn suggestions(today: NaiveDate) -> Suggestions {
    Suggestions {
        today,
        will_come: suggested_next(Outcome::WillCome, today),
        not_interested: suggested_next(Outcome::NotInterested, today),
        no_answer: suggested_next(Outcome::NoAnswer, today),
        other: suggested_next(Outcome::Other, today),
    }
}

struct Clean {
    contacted_on: NaiveDate,
    note: Option<String>,
    next: Option<NaiveDate>,
}

fn validate(input: &FollowUpInput, today: NaiveDate) -> Result<Clean, AppError> {
    let mut errors = Vec::new();
    let contacted_on = input.contacted_on.unwrap_or(today);
    if contacted_on > today {
        errors.push(FieldError::new("contacted_on", "in_future"));
    } else if today
        .checked_sub_days(Days::new(MAX_BACKDATE_DAYS))
        .is_some_and(|oldest| contacted_on < oldest)
    {
        errors.push(FieldError::new("contacted_on", "too_old"));
    }
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    if note
        .as_ref()
        .is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS)
    {
        errors.push(FieldError::new("note", "too_long"));
    }
    if let Some(Some(next)) = input.next_follow_up_on {
        if next < contacted_on {
            errors.push(FieldError::new("next_follow_up_on", "before_contact"));
        } else if contacted_on
            .checked_add_days(Days::new(MAX_SCHEDULE_DAYS))
            .is_some_and(|latest| next > latest)
        {
            errors.push(FieldError::new("next_follow_up_on", "too_far"));
        }
    }
    if !errors.is_empty() {
        return Err(AppError::Validation(errors));
    }
    Ok(Clean {
        contacted_on,
        note,
        next: next_follow_up(input.outcome, contacted_on, input.next_follow_up_on),
    })
}

/// `POST /customers/{id}/follow-ups` (`customer.follow_up`, Idempotency-Key). Returns the stored
/// response (201 and the follow-up as JSON); `to_body` renders the DTO so a replay is
/// byte-identical.
pub async fn record(
    pool: &PgPool,
    actor: &Actor,
    key: Uuid,
    request_hash: [u8; 32],
    customer_id: Uuid,
    input: &FollowUpInput,
    to_body: impl FnOnce(FollowUp) -> Result<Value, AppError>,
) -> Result<(u16, Value), AppError> {
    actor.require(Permission::CustomerFollowUp)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if let Claim::Replay { status, body } =
        idempotency::claim(&mut tx, actor, key, request_hash).await?
    {
        tx.commit().await?;
        return Ok((status, body));
    }
    if !repo::exists(&mut tx, customer_id).await? {
        return Err(AppError::NotFound);
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let c = validate(input, today)?;
    let id = Uuid::now_v7();
    repo::insert_follow_up(
        &mut tx,
        &FollowUpInsert {
            id,
            customer_id,
            contacted_on: c.contacted_on,
            channel: input.channel.as_str(),
            outcome: input.outcome.as_str(),
            note: c.note.as_deref(),
            next_follow_up_on: c.next,
            created_by: actor.user_id.as_uuid(),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        actor,
        AuditEntry {
            action: "customer.follow_up",
            entity_type: "customer",
            entity_id: customer_id,
            before: None,
            // The note is free text about a person: the audit says that one exists, not what.
            after: Some(json!({
                "follow_up_id": id, "contacted_on": c.contacted_on,
                "channel": input.channel.as_str(), "outcome": input.outcome.as_str(),
                "next_follow_up_on": c.next, "has_note": c.note.is_some(),
            })),
            reason: None,
        },
    )
    .await?;
    let row = repo::follow_up(&mut tx, id)
        .await?
        .ok_or_else(|| AppError::Internal("follow-up vanished".into()))?;
    let body = to_body(FollowUp::try_from(row)?)?;
    idempotency::complete(&mut tx, actor, key, 201, &body).await?;
    tx.commit().await?;
    Ok((201, body))
}

/// `GET /customers/{id}/follow-ups` (`customer.read`): newest contact first, at most
/// [`LIST_LIMIT`]; `bool` = more exist; with today's suggested next dates.
pub async fn list(
    pool: &PgPool,
    actor: &Actor,
    customer_id: Uuid,
) -> Result<(Vec<FollowUp>, bool, Suggestions), AppError> {
    actor.require(Permission::CustomerRead)?;
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    if !repo::exists(&mut tx, customer_id).await? {
        return Err(AppError::NotFound);
    }
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let mut rows = repo::follow_ups(&mut tx, customer_id, LIST_LIMIT + 1).await?;
    tx.commit().await?;
    let page = usize::try_from(LIST_LIMIT).unwrap_or(usize::MAX);
    let more = rows.len() > page;
    rows.truncate(page);
    let items = rows
        .into_iter()
        .map(FollowUp::try_from)
        .collect::<Result<_, _>>()?;
    Ok((items, more, suggestions(today)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    fn input(outcome: Outcome) -> FollowUpInput {
        FollowUpInput {
            contacted_on: None,
            channel: Channel::Phone,
            outcome,
            note: None,
            next_follow_up_on: None,
        }
    }

    #[test]
    fn no_answer_retries_after_three_days_unless_a_date_is_given() {
        let on = d("2026-11-05");
        assert_eq!(
            next_follow_up(Outcome::NoAnswer, on, None),
            Some(d("2026-11-08"))
        );
        assert_eq!(
            next_follow_up(Outcome::NoAnswer, on, Some(Some(d("2026-11-20")))),
            Some(d("2026-11-20"))
        );
        assert_eq!(
            next_follow_up(Outcome::WillCome, on, None),
            Some(d("2026-11-12")),
            "will come → check in a week"
        );
        for o in [Outcome::NotInterested, Outcome::Other] {
            assert_eq!(next_follow_up(o, on, None), None, "{o:?}");
        }
        assert_eq!(
            next_follow_up(Outcome::NotInterested, on, Some(Some(d("2027-01-05")))),
            Some(d("2027-01-05")),
            "a date given always wins"
        );
        for o in [Outcome::NoAnswer, Outcome::WillCome] {
            assert_eq!(
                next_follow_up(o, on, Some(None)),
                None,
                "{o:?}: \"Không hẹn\" is never replaced by the suggestion"
            );
        }
        assert_eq!(
            suggestions(on),
            Suggestions {
                today: on,
                will_come: Some(d("2026-11-12")),
                not_interested: None,
                no_answer: Some(d("2026-11-08")),
                other: None,
            }
        );
    }

    #[test]
    fn contact_day_is_today_or_recent_and_next_is_after_it() -> Result<(), AppError> {
        let today = d("2026-11-05");
        let c = validate(&input(Outcome::NoAnswer), today)?;
        assert_eq!((c.contacted_on, c.next), (today, Some(d("2026-11-08"))));

        let mut late = input(Outcome::Other);
        late.contacted_on = Some(d("2026-10-06"));
        assert!(validate(&late, today).is_ok(), "30 days back");
        late.contacted_on = Some(d("2026-10-05"));
        assert!(validate(&late, today).is_err(), "31 days back");
        late.contacted_on = Some(d("2026-11-06"));
        assert!(validate(&late, today).is_err(), "tomorrow");

        let mut next = input(Outcome::WillCome);
        next.next_follow_up_on = Some(Some(d("2026-11-04")));
        assert!(validate(&next, today).is_err(), "before the contact");
        next.next_follow_up_on = Some(Some(d("2027-11-06")));
        assert!(validate(&next, today).is_err(), "more than a year out");
        next.next_follow_up_on = Some(Some(today));
        assert!(validate(&next, today).is_ok(), "same day");

        let mut note = input(Outcome::Other);
        note.note = Some("  ".into());
        assert_eq!(validate(&note, today)?.note, None, "blank note dropped");
        note.note = Some("x".repeat(501));
        assert!(validate(&note, today).is_err());
        Ok(())
    }

    #[test]
    fn enums_round_trip() {
        for c in [
            Channel::Phone,
            Channel::Zalo,
            Channel::InPerson,
            Channel::Other,
        ] {
            assert_eq!(Channel::parse(c.as_str()), Some(c));
        }
        for o in [
            Outcome::WillCome,
            Outcome::NotInterested,
            Outcome::NoAnswer,
            Outcome::Other,
        ] {
            assert_eq!(Outcome::parse(o.as_str()), Some(o));
        }
        assert_eq!(Channel::parse("sms"), None);
    }
}
