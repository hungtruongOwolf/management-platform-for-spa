//! Cross-cutting HTTP protections applied to every route (threat model TB1).

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use governor::clock::QuantaInstant;
use governor::middleware::NoOpMiddleware;
use kernel::AppError;
use kernel::error::{PROBLEM_JSON, Problem};
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_governor::key_extractor::{KeyExtractor, SmartIpKeyExtractor};
use tower_governor::{GovernorError, GovernorLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone)]
pub struct HttpPolicy {
    pub public_origin: String,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    /// Sustained requests per second per client IP, and the burst allowed on top.
    pub rate_per_second: u64,
    pub rate_burst: u32,
}

impl HttpPolicy {
    pub fn standard(public_origin: String) -> Self {
        Self {
            public_origin,
            request_timeout: Duration::from_secs(10),
            max_body_bytes: 64 * 1024,
            rate_per_second: 5,
            rate_burst: 60,
        }
    }
}

/// The rate-limit bucket of a client address: an IPv4 address as is, an IPv6 address by its /64
/// (security review L1). One IPv6 subscriber usually holds a whole /64, so per-address buckets
/// would hand an attacker 2^64 fresh quotas. IPv4-mapped IPv6 counts as the IPv4 address.
pub fn client_bucket(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let [a, b, c, d, ..] = v6.segments();
                IpAddr::V6(Ipv6Addr::new(a, b, c, d, 0, 0, 0, 0))
            }
        },
    }
}

/// `SmartIpKeyExtractor` (X-Forwarded-For set by Caddy, then the peer address) bucketed by
/// [`client_bucket`].
#[derive(Debug, Clone, Copy)]
pub struct ClientBucketKeyExtractor;

impl KeyExtractor for ClientBucketKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        SmartIpKeyExtractor.extract(req).map(client_bucket)
    }
}

/// How often idle buckets are dropped from a limiter's in-memory map.
const LIMITER_SWEEP: Duration = Duration::from_secs(60);

/// Without a sweep, every client bucket ever seen stays in memory (security review L1: unbounded
/// growth under a spray of source addresses). The task stops once the layer is dropped.
type ClientLimiter = GovernorConfig<ClientBucketKeyExtractor, NoOpMiddleware<QuantaInstant>>;

fn spawn_limiter_sweep(config: &Arc<ClientLimiter>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let weak = Arc::downgrade(config);
    runtime.spawn(async move {
        let mut tick = tokio::time::interval(LIMITER_SWEEP);
        tick.tick().await;
        loop {
            tick.tick().await;
            let Some(config) = weak.upgrade() else { break };
            let limiter = config.limiter();
            limiter.retain_recent();
            limiter.shrink_to_fit();
        }
    });
}

fn client_limiter(replenish_ms: u64, burst: u32) -> Option<Arc<ClientLimiter>> {
    let config = GovernorConfigBuilder::default()
        .per_millisecond(replenish_ms)
        .burst_size(burst)
        .key_extractor(ClientBucketKeyExtractor)
        .finish()
        .map(Arc::new)?;
    spawn_limiter_sweep(&config);
    Some(config)
}

/// Wraps `router` with a private default `Cache-Control`, request ids, tracing, problem+json for bare errors, timeout,
/// per-IP rate limiting, a body size cap and the CSRF Origin check (outermost first).
pub fn apply(router: Router, policy: &HttpPolicy) -> Router {
    // tower_governor's `per_second(n)` is the *period* between tokens (n seconds), not a rate:
    // convert the rate into the replenish interval.
    let replenish_ms = (1000 / policy.rate_per_second.max(1)).max(1);
    let governor = client_limiter(replenish_ms, policy.rate_burst);
    let origin = Arc::new(policy.public_origin.clone());

    let router = router.layer(middleware::from_fn_with_state(origin, check_origin));
    let router = match governor {
        Some(config) => router.layer(GovernorLayer::new(config)),
        None => {
            tracing::error!("invalid rate limit policy; rate limiting disabled");
            router
        }
    };

    router.layer(
        ServiceBuilder::new()
            .layer(middleware::map_response(private_unless_set))
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(PropagateRequestIdLayer::x_request_id())
            .layer(TraceLayer::new_for_http().make_span_with(request_span))
            .layer(middleware::from_fn(problem_for_bare_errors))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                policy.request_timeout,
            ))
            .layer(RequestBodyLimitLayer::new(policy.max_body_bytes)),
    )
}

/// Stricter per-IP limit for `/auth/*` (credential stuffing): one request per 3 s sustained, burst 10
/// (≈ 20/min). Per-account lockout (identity::lockout) is the second layer.
pub fn with_auth_rate_limit<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    match client_limiter(3_000, 10) {
        Some(config) => router.layer(GovernorLayer::new(config)),
        None => {
            tracing::error!("invalid auth rate limit policy; auth rate limiting disabled");
            router
        }
    }
}

/// The span for one request: method and path only. The query string is never logged
/// (security review L5): `GET /customers?q=` carries customer names and phone numbers.
fn request_span<B>(request: &Request<B>) -> tracing::Span {
    tracing::debug_span!(
        "request",
        method = %request.method(),
        path = %request.uri().path(),
        version = ?request.version(),
    )
}

/// State-changing requests must come from our own SPA origin (CSRF defence with SameSite cookies).
async fn check_origin(
    State(allowed): State<Arc<String>>,
    request: Request,
    next: Next,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if safe {
        return next.run(request).await;
    }
    let origin_ok = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|origin| origin == allowed.as_str());
    if origin_ok {
        next.run(request).await
    } else {
        AppError::Forbidden {
            code: "auth.bad_origin",
        }
        .into_response()
    }
}

/// Default for every API response, errors and rate-limit rejections included.
pub const DEFAULT_CACHE_CONTROL: &str = "private, no-store";

/// Responses that set no `Cache-Control` would otherwise get the CDN's default
/// (`public, max-age=0, must-revalidate` on Vercel), which marks customer names, phone numbers and
/// salaries as shareable. Outermost layer, so it also covers responses built by inner layers
/// (429, 408, 413, problem+json fallbacks). A handler that sets its own value keeps it.
async fn private_unless_set(mut response: Response) -> Response {
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(DEFAULT_CACHE_CONTROL));
    response
}

/// Turns framework-generated errors (404 fallback, 405, 408, 413, 429, …) into problem+json,
/// so every error the client sees has the same shape and a stable `code`.
async fn problem_for_bare_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(PROBLEM_JSON.as_bytes()));
    if (status.is_client_error() || status.is_server_error()) && !is_problem {
        let mut replaced = Problem::for_status(status).into_response();
        if let Some(retry) = response.headers().get(header::RETRY_AFTER) {
            replaced
                .headers_mut()
                .insert(header::RETRY_AFTER, retry.clone());
        }
        return replaced;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn ip(s: &str) -> IpAddr {
        s.parse()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    }

    #[test]
    fn ipv4_clients_keep_their_own_bucket() {
        assert_eq!(client_bucket(ip("203.0.113.7")), ip("203.0.113.7"));
        assert_ne!(
            client_bucket(ip("203.0.113.7")),
            client_bucket(ip("203.0.113.8"))
        );
    }

    #[test]
    fn one_ipv6_slash_64_shares_one_bucket() {
        let a = client_bucket(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd"));
        let b = client_bucket(ip("2001:db8:1:2::1"));
        assert_eq!(a, ip("2001:db8:1:2::"));
        assert_eq!(a, b, "rotating the interface id gives no new quota");
        assert_ne!(
            a,
            client_bucket(ip("2001:db8:1:3::1")),
            "next /64 is someone else"
        );
    }

    #[test]
    fn ipv4_mapped_ipv6_counts_as_the_ipv4_address() {
        assert_eq!(client_bucket(ip("::ffff:203.0.113.7")), ip("203.0.113.7"));
    }

    fn request(forwarded_for: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri("/");
        if let Some(xff) = forwarded_for {
            builder = builder.header("x-forwarded-for", xff);
        }
        let mut req = builder.body(Body::empty()).unwrap_or_default();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [10, 0, 0, 2],
                443,
            ))));
        req
    }

    #[test]
    fn key_comes_from_the_proxy_header_then_the_peer() {
        let key = |r: Request<Body>| ClientBucketKeyExtractor.extract(&r).ok();
        assert_eq!(
            key(request(Some("2001:db8:1:2:3:4:5:6"))),
            Some(ip("2001:db8:1:2::"))
        );
        assert_eq!(key(request(None)), Some(ip("10.0.0.2")));
    }

    #[tokio::test]
    async fn idle_buckets_are_swept_and_the_sweep_stops_with_the_layer() {
        let Some(config) = client_limiter(1, 1) else {
            panic!("valid limiter policy");
        };
        let _ = config.limiter().check_key(&ip("203.0.113.7"));
        assert_eq!(config.limiter().len(), 1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        config.limiter().retain_recent();
        assert_eq!(config.limiter().len(), 0, "a refilled bucket is dropped");
        let weak = Arc::downgrade(&config);
        drop(config);
        assert!(
            weak.upgrade().is_none(),
            "the sweep task holds no strong reference"
        );
    }
}
