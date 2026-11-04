# syntax=docker/dockerfile:1.7
# Backup/restore toolbox: pg_dump/pg_restore 17 (dumps servers <= 17), age (encryption), rclone (Cloudflare R2, S3 API).
# Build context: infra/backup
#   docker build -t vpyd-backup infra/backup
# pg_dump must be at least the server's major version: bump postgresql17-client if Supabase runs a newer major.
FROM alpine:3.22
RUN apk add --no-cache postgresql17-client age rclone ca-certificates curl \
 && addgroup -S -g 10001 backup && adduser -S -u 10001 -G backup -h /work backup
COPY --chmod=0755 backup.sh restore.sh /usr/local/bin/
USER backup
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/backup.sh"]
