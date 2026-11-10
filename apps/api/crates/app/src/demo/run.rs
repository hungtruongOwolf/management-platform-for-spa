//! Bounded concurrency and retries for the demo seed. Staging reaches Postgres through a pooler at
//! ~150 ms a round trip, so a year of history recorded one use case after another would take
//! hours; independent work (separate customers, separate months, separate days) runs side by side
//! on several pool connections instead. Everything runs inside the caller's task (no spawn), so
//! the work may borrow and `server_timing::count_statements` sees every statement.

use futures_util::future::join_all;
use kernel::AppError;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Runs `work` on every item with at most `workers` in flight and returns the outputs in item
/// order. Items should come longest first: a worker takes the next item as soon as it is free.
/// The first error stops every worker from taking more and is returned.
pub async fn each<T, O, F, Fut>(items: Vec<T>, workers: usize, work: F) -> anyhow::Result<Vec<O>>
where
    F: Fn(T) -> Fut,
    Fut: Future<Output = anyhow::Result<O>>,
{
    let total = items.len();
    let queue = Mutex::new(items.into_iter().enumerate().collect::<VecDeque<_>>());
    let done = Mutex::new(Vec::with_capacity(total));
    let failed = AtomicBool::new(false);
    let worker = || async {
        loop {
            if failed.load(Ordering::Relaxed) {
                return Ok(());
            }
            let next = queue
                .lock()
                .map_err(|_| anyhow::anyhow!("work queue poisoned"))?
                .pop_front();
            let Some((i, item)) = next else {
                return Ok(());
            };
            // Boxed: a use case's future is large (debug builds especially); several of them
            // inline in one task overflow the 1 MiB main-thread stack on Windows.
            match Box::pin(work(item)).await {
                Ok(out) => done
                    .lock()
                    .map_err(|_| anyhow::anyhow!("results poisoned"))?
                    .push((i, out)),
                Err(e) => {
                    failed.store(true, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
    };
    let outcomes = join_all((0..workers.clamp(1, total.max(1))).map(|_| worker())).await;
    for outcome in outcomes {
        outcome?;
    }
    let mut done = done
        .into_inner()
        .map_err(|_| anyhow::anyhow!("results poisoned"))?;
    done.sort_by_key(|(i, _)| *i);
    Ok(done.into_iter().map(|(_, out)| out).collect())
}

/// Calls `call` again (up to four retries, with backoff) when it failed on something transient:
/// a dropped connection, a pool timeout, a deadlock or serialization failure. Every use case the
/// seed retries is idempotent (same idempotency key, or an upsert), so a write that committed
/// before its reply was lost is replayed, never doubled.
pub async fn retry<O, F, Fut>(mut call: F) -> Result<O, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<O, AppError>>,
{
    let mut attempt = 0_u32;
    loop {
        match call().await {
            Err(e) if attempt < 4 && transient(&e) => {
                attempt += 1;
                tracing::warn!(attempt, error = %e, "transient database error, retrying");
                tokio::time::sleep(Duration::from_millis(250 << attempt)).await;
            }
            other => return other,
        }
    }
}

fn transient(e: &AppError) -> bool {
    let AppError::Database(e) = e else {
        return false;
    };
    match e {
        sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut | sqlx::Error::WorkerCrashed => true,
        sqlx::Error::Database(db) => matches!(
            db.code().as_deref(),
            // serialization_failure, deadlock_detected, admin_shutdown, cannot_connect_now
            Some("40001" | "40P01" | "57P01" | "57P03")
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[tokio::test]
    async fn every_item_runs_once_and_outputs_keep_item_order() {
        let calls = AtomicUsize::new(0);
        let out = each((0..50).collect(), 7, |n: u32| {
            calls.fetch_add(1, Ordering::Relaxed);
            async move {
                tokio::time::sleep(Duration::from_millis(u64::from(50 - n) % 5)).await;
                Ok(n * 2)
            }
        })
        .await;
        assert_eq!(calls.load(Ordering::Relaxed), 50);
        assert_eq!(out.ok(), Some((0..50).map(|n| n * 2).collect::<Vec<u32>>()));
    }

    #[tokio::test]
    async fn the_first_error_is_returned() {
        let out = each(vec![1, 2, 3], 2, |n: u32| async move {
            anyhow::ensure!(n != 2, "item {n} failed");
            Ok(n)
        })
        .await;
        assert!(out.is_err_and(|e| e.to_string().contains("item 2")));
    }

    #[tokio::test]
    async fn only_transient_errors_are_retried() {
        let calls = AtomicUsize::new(0);
        let refused: Result<(), AppError> = retry(|| {
            calls.fetch_add(1, Ordering::Relaxed);
            async { Err(AppError::NotFound) }
        })
        .await;
        assert!(refused.is_err());
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "a business refusal is final"
        );

        let calls = AtomicUsize::new(0);
        let recovered = retry(|| {
            let n = calls.fetch_add(1, Ordering::Relaxed);
            async move {
                if n == 0 {
                    Err(AppError::Database(sqlx::Error::PoolTimedOut))
                } else {
                    Ok(n)
                }
            }
        })
        .await;
        assert_eq!(recovered.ok(), Some(1));
    }
}
