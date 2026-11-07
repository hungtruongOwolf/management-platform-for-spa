//! `Server-Timing` on every API response (perf page §4d): where a request's time goes, readable in
//! the browser's DevTools and by `infra/perf/endpoints.mjs`, without logs or a database login.
//!
//! `db` is the sum of the SQL statements the request ran and how many there were. sqlx reports each
//! statement as a `sqlx::query` tracing event carrying `elapsed_secs`; [`DbTimingLayer`] adds it to
//! the request's task-local tally. A statement run outside a request (jobs, startup) has no tally
//! and is ignored. `total` is the time inside the API, from the first layer to the response head.
//!
//! Not on `/auth/*`: a statement count per login attempt would tell an unknown phone from a
//! locked or a known one more precisely than the response time does.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use std::cell::Cell;
use std::time::{Duration, Instant};
use tracing::field::{Field, Visit};
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, Filter};

const SQLX_TARGET: &str = "sqlx::query";
const HEADER: &str = "server-timing";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct DbTally {
    statements: u32,
    elapsed: Duration,
}

tokio::task_local! {
    static DB: Cell<DbTally>;
}

/// Adds one finished statement to the current request's tally (no-op outside a request).
fn record_statement(elapsed: Duration) {
    let _ = DB.try_with(|tally| {
        let mut t = tally.get();
        t.statements = t.statements.saturating_add(1);
        t.elapsed = t.elapsed.saturating_add(elapsed);
        tally.set(t);
    });
}

/// Tracing layer that only listens to sqlx's per-statement events. Install it next to the log
/// layer; its own filter enables `sqlx::query` at DEBUG for itself only, so the log output keeps
/// whatever level `RUST_LOG` chose.
pub struct DbTimingLayer;

impl DbTimingLayer {
    /// The layer with its per-layer filter, ready for `registry().with(...)`.
    pub fn filtered<S>() -> tracing_subscriber::filter::Filtered<Self, SqlxStatements, S>
    where
        S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        Self.with_filter(SqlxStatements)
    }
}

/// Per-layer filter: sqlx statement events and nothing else.
pub struct SqlxStatements;

impl<S> Filter<S> for SqlxStatements {
    fn enabled(&self, meta: &Metadata<'_>, _: &Context<'_, S>) -> bool {
        meta.target() == SQLX_TARGET
    }
}

struct ElapsedSecs(Option<f64>);

impl Visit for ElapsedSecs {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.0 = Some(value);
        }
    }
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {}
}

impl<S: Subscriber> Layer<S> for DbTimingLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut visitor = ElapsedSecs(None);
        event.record(&mut visitor);
        if let Some(secs) = visitor.0.filter(|s| s.is_finite() && *s >= 0.0) {
            record_statement(Duration::from_secs_f64(secs));
        }
    }
}

fn millis(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

fn header_value(db: DbTally, total: Duration) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!(
        "db;dur={};desc=\"{} statements\", total;dur={}",
        millis(db.elapsed),
        db.statements,
        millis(total)
    ))
    .ok()
}

/// Middleware: runs the request inside a fresh tally and writes `Server-Timing` on the way out.
pub async fn server_timing(request: Request, next: Next) -> Response {
    if request.uri().path().contains("/auth/") {
        return next.run(request).await;
    }
    let started = Instant::now();
    let (mut response, tally) = DB
        .scope(Cell::new(DbTally::default()), async move {
            let response = next.run(request).await;
            (response, DB.with(Cell::get))
        })
        .await;
    if let Some(value) = header_value(tally, started.elapsed()) {
        response.headers_mut().insert(HEADER, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_lists_db_time_statement_count_and_total() {
        let value = header_value(
            DbTally {
                statements: 7,
                elapsed: Duration::from_micros(1_234),
            },
            Duration::from_micros(5_678),
        );
        assert_eq!(
            value.as_ref().and_then(|v| v.to_str().ok()),
            Some("db;dur=1.23;desc=\"7 statements\", total;dur=5.68")
        );
    }

    #[tokio::test]
    async fn statements_count_only_inside_a_request() {
        record_statement(Duration::from_millis(5));
        let tally = DB
            .scope(Cell::new(DbTally::default()), async {
                record_statement(Duration::from_millis(2));
                record_statement(Duration::from_millis(3));
                DB.with(Cell::get)
            })
            .await;
        assert_eq!(
            tally,
            DbTally {
                statements: 2,
                elapsed: Duration::from_millis(5)
            }
        );
    }
}
