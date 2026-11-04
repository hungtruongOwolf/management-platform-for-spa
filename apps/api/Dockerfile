# syntax=docker/dockerfile:1.7
# Production image for the Rust API (`app` binary). Build context: apps/api
#   docker build -t vpyd-api apps/api
# Long-running:  app serve    (APP_DATABASE_URL = app_rw, row-level security applies)
# One-shot job:  app migrate  (DATABASE_URL = schema owner; runs before serve takes traffic)

ARG RUST_VERSION=1.99

# --- Dependency caching (cargo-chef): dependencies rebuild only when Cargo.toml / Cargo.lock change.
FROM lukemathwalker/cargo-chef:latest-rust-${RUST_VERSION}-slim-trixie AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# Queries are checked against the committed .sqlx data; no database is reachable at build time.
ENV SQLX_OFFLINE=true \
    CARGO_TERM_COLOR=never
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --locked -p app --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked -p app \
 && strip target/release/app \
 && cp target/release/app /app

# Static busybox only provides `wget` for the HEALTHCHECK (the runtime image has no shell or curl).
FROM busybox:1.37-musl AS probe

# --- Runtime: distroless (glibc + CA certificates, no shell, no package manager), non-root uid 65532.
FROM gcr.io/distroless/cc-debian13:nonroot AS runtime
COPY --from=probe /bin/busybox /usr/local/bin/busybox
COPY --from=builder /app /usr/local/bin/app
ENV APP_BIND_ADDR=0.0.0.0:8080 \
    APP_ENV=production \
    RUST_LOG=info
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/busybox", "wget", "-q", "-O", "/dev/null", "http://127.0.0.1:8080/api/v1/health"]
ENTRYPOINT ["/usr/local/bin/app"]
CMD ["serve"]
