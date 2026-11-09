//! The runtime pool (`app_rw`). One request checks a connection out of the pool up to three times
//! (session, permissions, the use case), and sqlx's default `test_before_acquire` pings on every
//! checkout: one extra round trip each, ~2.6 ms on staging (db review 2026-11, F2). Here a
//! connection is pinged only when it has been idle for a while, which is when a dead one is
//! likely: the instance was frozen (Vercel scale-to-zero), the pooler or Postgres closed it, the
//! network dropped. sqlx documents this pattern on `PoolOptions::before_acquire`.

use sqlx::Connection;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// A connection idle longer than this is pinged before it is handed out. Shorter than any idle
/// timeout that closes connections on the way (Supavisor, Postgres, NAT), and shorter than the
/// ~45 s gap after which a Vercel instance was measured to come back cold (perf page §2e).
pub const PING_WHEN_IDLE_FOR: Duration = Duration::from_secs(30);

/// Connections idle this long are closed by the pool's reaper instead of being kept.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Whether a connection idle for `idle` must prove it is alive before use.
pub fn needs_ping(idle: Duration, threshold: Duration) -> bool {
    idle >= threshold
}

/// Pool options for the API: ping only connections idle for at least `ping_when_idle_for`.
/// A ping that fails closes the connection and the pool hands out another one (or opens a new
/// one), so the request never sees the dead connection.
pub fn options(max_connections: u32, ping_when_idle_for: Duration) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(IDLE_TIMEOUT)
        .test_before_acquire(false)
        .before_acquire(move |conn, meta| {
            Box::pin(async move {
                if needs_ping(meta.idle_for, ping_when_idle_for) {
                    conn.ping().await?;
                }
                Ok(true)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_connection_idle_for_the_threshold_is_pinged() {
        let t = PING_WHEN_IDLE_FOR;
        assert!(!needs_ping(Duration::ZERO, t));
        assert!(!needs_ping(t - Duration::from_millis(1), t));
        assert!(needs_ping(t, t));
        assert!(needs_ping(Duration::from_secs(600), t));
        assert!(
            needs_ping(Duration::ZERO, Duration::ZERO),
            "threshold 0 = ping always"
        );
    }
}
