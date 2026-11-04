---
title: Runbook — dựng staging (VPS cùng vùng Supabase + R2)
updated: 2026-11-04
sources: [ADR-0002, ADR-0005, architecture/system.md, plans/r1-1-front-desk.md (S11)]
---

# Runbook: dựng staging S11

Mục tiêu: một VPS chạy `caddy` + `api` bằng Docker Compose, database là Supabase project `vpyd-staging` (ref `zyyiukcysjntjgjabiqe`), backup đêm mã hóa lên Cloudflare R2, có uptime alert.

> **Quyết định EM (2026-11-04):** project staging nằm ở **vùng Mỹ** (vùng cụ thể đọc từ pooler host `aws-<index>-<region>.pooler.supabase.com`) và **Data API vẫn bật**. Do đó (1) VPS staging phải đặt **cùng vùng với DB**: đi xuyên Thái Bình Dương mất khoảng 170 ms mỗi query, không chấp nhận được; (2) giảm thiểu rủi ro Data API bằng REVOKE (§4.2). **Production** sẽ là một project Supabase **mới ở Singapore** + VPS Singapore theo ADR-0002/0005; bảng Singapore ở §1.2 dành cho lúc đó. Chỉ dùng **dữ liệu giả** cho tới khi xong thủ tục chuyển dữ liệu ra nước ngoài (ADR-0002, Luật 91/2025/QH15).

Thời gian ước tính sau khi EM đã có tài khoản: khoảng 1 giờ.

## 0. Bản đồ file trong repo

| File | Vai trò |
|---|---|
| `apps/api/Dockerfile` | Image API: cargo-chef cache, `SQLX_OFFLINE=true`, release, runtime **distroless nonroot** (uid 65532), `HEALTHCHECK` gọi `/api/v1/health` bằng busybox `wget` tĩnh |
| `infra/Caddy.Dockerfile` + `infra/Caddyfile` | Image web: build SPA bằng pnpm rồi copy vào Caddy. TLS tự động theo `{$DOMAIN}`, CSP chặt, HSTS, SPA fallback, `/assets/*` cache `immutable`, `/api/*` proxy sang `api:8080` |
| `infra/docker-compose.prod.yml` | `migrate` (one-shot, role owner), `api` (chỉ có URL `app_rw`), `caddy` (80/443); profile `ops`: `backup`; profile `drill`: `restore-db` + `restore` |
| `infra/.env.production.example` | Danh sách mọi biến, không có giá trị. Bản thật `/opt/vpyd/.env.production` chỉ nằm trên VPS (`chmod 600`) |
| `infra/backup/` | Image backup (pg_dump 17 + age + rclone), `backup.sh`, `restore.sh`, systemd `vpyd-backup.service/.timer` |
| `infra/deploy.sh` | Deploy/rollback trên server: pull → `migrate` → `api` healthy → `caddy` → smoke check |
| `.github/workflows/deploy.yml` | `workflow_dispatch`: build + push GHCR, rồi SSH deploy. Secret nằm trong GitHub environment `staging` |
| `infra/prodtest/` | Smoke test cả stack ở local (Postgres giả thay Supabase, backup ra volume local) |

Nguyên tắc: container `api` **không bao giờ** nhận URL của owner. `--env-file` chỉ dùng để nội suy, nên mỗi container chỉ thấy đúng các biến khai trong `environment:` của nó (đã kiểm tra bằng `docker compose config`).

## 1. Chọn VPS

### 1.1 Staging: cùng vùng với Supabase (Mỹ)

Ghép vùng: VPS phải nằm cùng metro với vùng AWS mà Supabase đang chạy. Các nhà cung cấp dưới đây (trừ Hetzner) **không chênh giá theo vùng**, nên giá ở Mỹ bằng giá ở bảng §1.2.

| Supabase region (pooler host) | AWS | VPS khớp nhất | RTT ước tính |
|---|---|---|---|
| `us-east-1` | N. Virginia | DigitalOcean **NYC3**/NYC1; Vultr New Jersey; Hetzner Ashburn (cùng metro) | ~5–8 ms (NYC), ~1–2 ms (Ashburn) |
| `us-east-2` | Ohio | DigitalOcean NYC3 / TOR1; Vultr Chicago | ~10–15 ms |
| `us-west-1` | N. California | DigitalOcean **SFO3**; Vultr Silicon Valley; Linode Fremont | ~2–5 ms |
| `us-west-2` | Oregon | Hetzner Hillsboro; Vultr Seattle; DigitalOcean SFO3 | ~2 ms (Hillsboro), ~20 ms (SFO) |

| Nhà cung cấp (Mỹ) | Gói | Giá/tháng | Traffic | Ghi chú |
|---|---|---|---|---|
| **DigitalOcean** NYC/SFO | Basic 1 vCPU / 2 GiB | **$12** | 2.000 GiB | cùng bảng giá mọi vùng. [DO pricing](https://www.digitalocean.com/pricing/droplets) |
| Akamai/Linode Newark/Fremont | Linode 2 GB | $12 | 2 TB | [Akamai CPU pricing](https://www.akamai.com/cloud/pricing/cpu) |
| Vultr NJ/Chicago/Silicon Valley/Seattle | 1 vCPU / 2 GB | ~$10–12 | 2 TB | số từ bên thứ ba (xem §1.2) |
| Hetzner Ashburn/Hillsboro | CPX11 2 vCPU / 2 GB | **$21,09** (€17,99) | 1 TB | đắt gần gấp đôi sau đợt tăng giá 2026; tài khoản mới có thể bị hạn chế tạo server. [agentdeals tổng hợp](https://agentdeals.dev/hetzner-pricing-2026) |

**Khuyến nghị staging: DigitalOcean Basic 2 GiB / 1 vCPU ($12/tháng) ở datacenter khớp vùng Supabase**: `us-east-1`/`us-east-2` → **NYC3**, `us-west-1`/`us-west-2` → **SFO3**. Lý do giống §1.2 (giá xác minh được, Cloud Firewall miễn phí). Hetzner Ashburn/Hillsboro có độ trễ thấp nhất, nhưng $21 thì không đáng cho staging. Sau khi dựng xong, đo RTT bằng `psql` với `\timing` + `select 1` từ VPS: kỳ vọng < 10 ms.

### 1.2 Production (sau này): Singapore

Giá kiểm tra ngày 2026-11-04. Trang giá chính thức có số rõ ràng chỉ với DigitalOcean và Akamai/Linode. Trang Vultr chặn bot (403) nên số của Vultr lấy từ bên thứ ba.

| Nhà cung cấp | Gói phù hợp | Giá/tháng | Traffic | Ghi chú |
|---|---|---|---|---|
| **DigitalOcean** (SGP1) | Basic 1 vCPU / 2 GiB / 50 GiB SSD | **$12** | 2.000 GiB | Có giá chính thức, không chênh giá theo vùng. Cloud Firewall miễn phí, có IPv6. Backup add-on +20% (weekly). [DO pricing](https://www.digitalocean.com/pricing/droplets) |
| Akamai/Linode (Singapore) | Linode 2 GB, 1 vCPU / 50 GB | $12 | 2 TB | Có giá chính thức, nhưng trang ghi "some plans differ by region". [Akamai CPU pricing](https://www.akamai.com/cloud/pricing/cpu) |
| Vultr (Singapore) | Regular/HF 1 vCPU / 2 GB | ~$10–12 | 2 TB | Chỉ có số từ bên thứ ba, các nguồn lệch nhau. [vpsbenchmarks](https://www.vpsbenchmarks.com/hosters/vultr/plans/49), [cloud-mercato](https://pcr.cloud-mercato.com/providers/vultr/flavors/vhf-1c-2gb) |
| Hetzner (Singapore) | — | đắt hơn EU | **0,5 TB** | Tăng giá 30–37% từ 04/2026, tăng tiếp từ 15/06/2026. CPX11–51 **ngừng bán ở Singapore**, Singapore "priced higher", traffic chỉ 0,5 TB rồi tính €7,40/TB. [agentdeals tổng hợp](https://agentdeals.dev/hetzner-pricing-2026), [Hetzner press](https://www.hetzner.com/pressroom/standardization-and-price-adjustment-of-our-server-products/), [northflank](https://northflank.com/blog/hetzner-cloud-server-price-increases) |

Tham khảo thêm (bên thứ ba, độ tin cậy thấp): [bitdoze DO vs Vultr vs Hetzner](https://www.bitdoze.com/digitalocean-vs-vultr-vs-hetzner/), [valebyte Singapore VPS](https://valebyte.com/en/blog/singapore-vps-vultr-vs-linode-vs-valebyte/). Nguồn tiếng Trung tìm được chủ yếu dẫn lại các bài trên và không có bảng giá Singapore riêng.

**Khuyến nghị production: DigitalOcean Basic 2 GiB / 1 vCPU, vùng SGP1, Ubuntu 24.04 LTS, $12/tháng**, đi kèm một project Supabase mới ở `ap-southeast-1`. Lý do:
- Giá xác minh được trên trang chính thức.
- Supabase `ap-southeast-1` chạy trên AWS Singapore, cùng thành phố với SGP1 nên độ trễ chỉ khoảng 1–2 ms.
- Cloud Firewall miễn phí là lớp tường lửa thứ hai ngoài `ufw`.
- 2 GiB dư cho `api` (≤ 384 MiB) + `caddy` (≤ 128 MiB) + backup. Image build trên GitHub Actions nên VPS không phải compile Rust.
- Gói 1 GiB ($6) cũng chạy được nếu thêm swap, nhưng sát giới hạn khi backup chạy cùng lúc với pull image.

Linode 2 GB cùng giá, là phương án dự phòng tương đương.

## 2. Domain và DNS
- Mua domain (ví dụ `.vn` qua nhà đăng ký trong nước, hoặc `.com` qua Cloudflare Registrar theo giá gốc). Staging dùng subdomain, ví dụ `staging.<domain>`.
- Tạo bản ghi `A` (và `AAAA` nếu VPS có IPv6) trỏ `staging.<domain>` về IP của VPS. Nếu DNS ở Cloudflare thì để **DNS only** (mây xám): Caddy tự lấy chứng chỉ Let's Encrypt và TLS đi thẳng tới VPS.
- Bản ghi `CAA` (tùy chọn): `0 issue "letsencrypt.org"`.

## 3. Hardening server (Ubuntu 24.04)

Chạy bằng root qua console hoặc SSH key mà nhà cung cấp gắn lúc tạo máy:

```bash
# user deploy, chỉ đăng nhập bằng SSH key
adduser --disabled-password --gecos "" deploy
install -d -m 700 -o deploy -g deploy /home/deploy/.ssh
cp /root/.ssh/authorized_keys /home/deploy/.ssh/ && chown deploy:deploy /home/deploy/.ssh/authorized_keys
usermod -aG sudo deploy   # cần đặt mật khẩu sudo: passwd deploy (mật khẩu chỉ để sudo, không dùng cho SSH)

cat >/etc/ssh/sshd_config.d/10-hardening.conf <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
PubkeyAuthentication yes
AllowUsers deploy
MaxAuthTries 3
EOF
sshd -t && systemctl reload ssh      # GIỮ phiên hiện tại, mở phiên mới thử `ssh deploy@IP` trước khi thoát

# firewall: chỉ 22, 80, 443 (tcp + udp cho HTTP/3)
ufw default deny incoming && ufw default allow outgoing
ufw limit 22/tcp && ufw allow 80/tcp && ufw allow 443/tcp && ufw allow 443/udp
ufw enable

# cập nhật bảo mật tự động, reboot lúc 04:00 nếu kernel đổi
apt update && apt -y upgrade && apt -y install unattended-upgrades fail2ban
dpkg-reconfigure -f noninteractive unattended-upgrades
cat >/etc/apt/apt.conf.d/52vpyd <<'EOF'
Unattended-Upgrade::Automatic-Reboot "true";
Unattended-Upgrade::Automatic-Reboot-Time "04:00";
EOF
timedatectl set-timezone Asia/Ho_Chi_Minh   # systemd timer backup ghi giờ UTC nên không bị ảnh hưởng

# fail2ban cho sshd
cat >/etc/fail2ban/jail.d/sshd.local <<'EOF'
[sshd]
enabled = true
maxretry = 5
bantime = 1h
EOF
systemctl enable --now fail2ban

# swap 1 GiB (đệm khi pull image)
fallocate -l 1G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab
```

**Docker** (repo apt chính thức, theo docs.docker.com/engine/install/ubuntu):
```bash
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo $VERSION_CODENAME) stable" > /etc/apt/sources.list.d/docker.list
apt update && apt -y install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
usermod -aG docker deploy      # lưu ý: nhóm docker tương đương root, nên chỉ deploy có quyền này
cat >/etc/docker/daemon.json <<'EOF'
{ "log-driver": "json-file", "log-opts": { "max-size": "10m", "max-file": "5" }, "live-restore": true }
EOF
systemctl restart docker
install -d -m 750 -o deploy -g deploy /opt/vpyd /opt/vpyd/infra /opt/vpyd/secrets
```

- **Docker và ufw:** port publish bởi Docker đi vòng qua ufw. Stack chỉ publish 80/443 (đằng nào cũng mở) và **không** publish port của `api`, nên không có lỗ hổng. Không thêm `ports:` cho `api`.
- **DigitalOcean Cloud Firewall** (lớp 2): inbound chỉ cho 80/443 từ mọi nơi và 22 từ IP của EM (nếu IP cố định) cộng IP runner GitHub. Nếu không cố định được IP SSH thì để 22 mở, vì đã có key-only + fail2ban.

## 4. Supabase (project `vpyd-staging`, ref `zyyiukcysjntjgjabiqe`)

### 4.1 Những gì EM bấm khi tạo project (đã làm cho staging)
- Org **"For my future"** (đăng nhập bằng GitHub, credits Pro đã redeem). Bật **MFA** cho tài khoản GitHub và tài khoản Supabase. Nên có thêm một owner thứ hai, ghi lại offline.
- Project name `vpyd-staging`, compute mặc định. Region: staging thực tế ở **Mỹ** (theo quyết định EM). Project production sau này chọn **Southeast Asia (Singapore)**.
- **Database password**: dùng nút Generate, lưu ngay vào password manager. EM đang rotate mật khẩu này. Agent không bao giờ nhận nó.
- **Data API**: staging **vẫn bật** (quyết định EM). Giảm thiểu bằng §4.2 bước 1 + 6. Production: bỏ chọn "Enable Data API" ngay khi tạo.

### 4.2 Checklist sau khi tạo (EM hoặc agent chạy, từ VPS qua `psql` để mật khẩu không vào lịch sử SQL Editor)

> Staging: lead đang tự bootstrap DB (role `app_rw`, migration, tenant). Checklist này dùng để **kiểm tra lại** sau khi bootstrap, và làm nguyên vẹn cho production.

Cài client trên VPS: `docker run --rm -it postgres:17-alpine psql "<owner-url>"`, hoặc `apt install postgresql-client-17` từ repo PGDG. Owner URL gõ tay trong phiên đó, không lưu vào file nào ngoài `.env.production`.

1. **Data API / PostgREST:** production thì tắt (Project Settings → Data API). Staging vẫn bật, nên: **Exposed schemas không được chứa `app`** (chỉ để `public` + `graphql_public` mặc định, hoặc rỗng), cộng thêm các REVOKE ở bước 6. Kiểm tra bằng anon key (key này công khai, không phải secret):
   `curl -s -H "apikey: $ANON" -H 'Accept-Profile: app' https://zyyiukcysjntjgjabiqe.supabase.co/rest/v1/tenant` → phải lỗi (schema không được expose / permission denied), không bao giờ trả dữ liệu. Thử thêm `/rest/v1/_sqlx_migrations` → `401/404/permission denied`.
2. **Auth không dùng:** Authentication → Sign In / Providers → tắt "Allow new users to sign up", để anon key không tạo được user.
3. **SSL:** Database Settings → SSL Configuration → bật **Enforce SSL on incoming connections** (sẽ reboot nhanh). Tải **CA certificate** → `/opt/vpyd/secrets/supabase-ca.crt` (`chmod 640`, owner deploy). Mọi URL dùng `sslmode=verify-full&sslrootcert=/run/secrets/supabase-ca.crt`.
4. **Role runtime `app_rw`** (giống `infra/postgres-init/01-roles.sql`, nhưng mật khẩu mạnh và không nằm trong SQL):
   ```sql
   CREATE ROLE app_rw LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS CONNECTION LIMIT 20;
   \password app_rw
   -- mật khẩu: tạo trên VPS bằng `openssl rand -base64 32 | tr -d '/+=' | cut -c1-32`, dán vào prompt và vào .env.production
   ```
   Không cần GRANT bằng tay: migration `0001+` tự cấp quyền tối thiểu (USAGE trên schema `app`, DML theo từng bảng, `audit_log` chỉ có INSERT/SELECT).
5. **Chạy migration lần đầu** (deploy ở §6 tự làm bước này): `migrate` chạy bằng role `postgres`, tạo schema `app` và bảng `public._sqlx_migrations`.
6. **Thu quyền của `anon`/`authenticated`** (bắt buộc khi Data API bật; Supabase có default privileges cấp cho hai role này). Chạy bằng role `postgres` sau migration đầu tiên. Default privileges lo phần bảng mới do `postgres` tạo, nhưng vẫn chạy lại câu kiểm tra ở bước 7 sau mỗi migration:
   ```sql
   REVOKE ALL ON SCHEMA app FROM anon, authenticated;
   REVOKE ALL ON ALL TABLES IN SCHEMA app FROM anon, authenticated;
   REVOKE ALL ON ALL SEQUENCES IN SCHEMA app FROM anon, authenticated;
   REVOKE ALL ON ALL FUNCTIONS IN SCHEMA app FROM anon, authenticated;
   ALTER DEFAULT PRIVILEGES IN SCHEMA app REVOKE ALL ON TABLES FROM anon, authenticated;
   ALTER DEFAULT PRIVILEGES IN SCHEMA app REVOKE ALL ON SEQUENCES FROM anon, authenticated;
   ALTER DEFAULT PRIVILEGES IN SCHEMA app REVOKE ALL ON FUNCTIONS FROM anon, authenticated;
   REVOKE ALL ON public._sqlx_migrations FROM anon, authenticated;   -- public LÀ schema được expose
   ```
7. **Kiểm tra RLS và quyền** (kỳ vọng ghi bên cạnh):
   ```sql
   -- bảng trong app chưa bật RLS → so với migration (bảng toàn cục như tenant được ghi chú trong data.md)
   SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
    WHERE n.nspname='app' AND c.relkind='r' AND NOT c.relrowsecurity;
   -- anon/authenticated/service_role không có quyền gì trên app → 0 dòng
   SELECT grantee, table_name, privilege_type FROM information_schema.role_table_grants
    WHERE table_schema='app' AND grantee IN ('anon','authenticated','service_role');
   -- app_rw không superuser, không bypass RLS → f | f
   SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname='app_rw';
   -- đăng nhập bằng app_rw, chưa set app.tenant_id → 0 dòng hoặc lỗi từ app.current_tenant() (RLS chặn), không bao giờ ra dữ liệu
   SELECT count(*) FROM app.branch;
   ```
8. **Network Restrictions** (chỉ áp dụng cho kết nối Postgres, **không** chặn Data API qua HTTPS, nên bước 6 vẫn bắt buộc): Database Settings → Network Restrictions → chỉ cho phép `<IP VPS>/32` (thêm `<IPv6>/128` nếu dùng direct). Khi EM cần kết nối từ máy cá nhân thì thêm tạm IP rồi xóa đi. Giới hạn này áp dụng cho cả direct lẫn pooler.
9. **Chọn kết nối cho SQLx** (theo [Supabase: Connect to your database](https://supabase.com/docs/guides/database/connecting-to-postgres)):
   - **Dùng: Supavisor session mode**: host `aws-<index>-<region>.pooler.supabase.com` (staging: một vùng `us-*`; production: `ap-southeast-1`; copy từ nút **Connect**, vì index không tự suy ra được), port **5432**, user **`app_rw.zyyiukcysjntjgjabiqe`** (role + `.` + ref; owner là `postgres.zyyiukcysjntjgjabiqe`). Chạy IPv4, hỗ trợ prepared statements.
   - Hoặc **direct**: `db.zyyiukcysjntjgjabiqe.supabase.co:5432`, user `app_rw`. Chỉ có IPv6 (Docker phải bật IPv6), trừ khi mua IPv4 add-on ($4/tháng).
   - **Không dùng transaction mode (6543):** SQLx dùng prepared statements và cache chúng theo connection. Transaction mode không hỗ trợ prepared statements, nên muốn dùng thì phải tắt statement cache (`statement_cache_capacity(0)`) và chấp nhận các giới hạn về session state. Không đáng với tải < 10 người dùng. `TenantTx` dùng `SET LOCAL` trong transaction nên chạy đúng ở session mode.
   - Pool: API mở tối đa 10 connection. Kiểm tra Database Settings → Connection pooling → Pool Size ≥ 15 (10 cho API, cộng migrate, backup và psql).
10. **Backup của Supabase:** Pro có daily backup 7 ngày. PITR là add-on trả phí, chưa cần vì đã có backup độc lập ra R2 (§7).

### 4.3 Connection string để ở đâu
- **Chỉ** ở `/opt/vpyd/.env.production` trên VPS (`chmod 600`, owner `deploy`). EM tự mở file bằng `ssh deploy@VPS` + `nano` rồi dán vào. Mật khẩu `app_rw` tạo ngay trên VPS (§4.2 bước 4) nên không cần đi qua đâu khác.
- Bản sao để khôi phục: password manager của EM.
- GitHub environment `staging` chỉ giữ thông tin SSH (§6). Nếu sau này muốn render `.env.production` từ CI thì chuyển các biến sang environment secrets, không bao giờ để trong repo.
- **Không bao giờ** dán vào chat, issue, PR, commit, Slack, hay SQL Editor snippet. Lỡ lộ thì rotate ngay (Database Settings → Reset password; `\password app_rw`).

## 5. Cloudflare R2 (backup)
1. R2 → Create bucket `vpyd-backups`, location hint **Asia-Pacific**.
2. **Lifecycle rules:** prefix `staging/daily/` xóa sau 35 ngày; `staging/monthly/` xóa sau 400 ngày. Script để `BACKUP_*_RETENTION_DAYS` trống, việc xóa do R2 làm.
3. **Bucket lock rule** (chống ransomware: VPS bị chiếm cũng không xóa được backup): prefix `staging/`, retention 30 ngày.
4. **API token:** R2 → Manage API tokens → "Object Read & Write", **chỉ bucket `vpyd-backups`**. Ghi lại Access Key ID và Secret (Secret chỉ hiện một lần), dán thẳng vào `.env.production`.
5. **Khóa mã hóa (age):** EM tạo trên máy cá nhân `age-keygen -o vpyd-backup-key.txt`. Dòng `# public key: age1…` là `AGE_RECIPIENT`, công khai được. File private key cất trong password manager kèm một bản offline (USB/giấy) và **không** để trên VPS, trừ lúc diễn tập khôi phục.

## 6. Deploy lần đầu
1. GitHub → Settings → Environments → `staging` (bật "Required reviewers" là EM). Secrets:
   - `STAGING_SSH_HOST` = IP VPS
   - `STAGING_SSH_KEY` = private key ed25519 **riêng cho deploy** (`ssh-keygen -t ed25519 -f vpyd-deploy -C gha-deploy`; public key thêm vào `/home/deploy/.ssh/authorized_keys`)
   - `STAGING_SSH_KNOWN_HOSTS` = `ssh-keyscan -t ed25519 <IP>`, đối chiếu fingerprint với console của nhà cung cấp
2. Trên VPS: `/opt/vpyd/.env.production` (từ `infra/.env.production.example`, `chmod 600`) và `/opt/vpyd/secrets/supabase-ca.crt`. Tạo `APP_DATA_KEY` bằng `openssl rand -base64 32` và lưu thêm một bản trong password manager, **tách riêng** khỏi backup DB.
3. Actions → **Deploy (staging)** → Run workflow. Workflow build 3 image, push lên `ghcr.io/hungtruongowolf/vpyd-*:<sha>`, đồng bộ `infra/` lên `/opt/vpyd/infra`, rồi chạy `infra/deploy.sh <sha>`. Token GHCR là token ngắn hạn của job và bị logout ngay sau khi pull, nên trên VPS không còn credential lâu dài nào.
4. Tạo spa và tài khoản thử (dữ liệu giả). Mật khẩu đọc từ env nên không lộ trong argv hay history:
   ```bash
   cd /opt/vpyd; C="docker compose --env-file .env.production -f infra/docker-compose.prod.yml"
   $C run --rm migrate create-tenant --slug vien-phuc-y-duong --name "Viên Phúc Y Đường" --branch "Cơ sở chính"
   $C run --rm migrate seed-catalog --tenant vien-phuc-y-duong
   read -rs APP_NEW_USER_PASSWORD && export APP_NEW_USER_PASSWORD
   $C run --rm -e APP_NEW_USER_PASSWORD migrate add-user --tenant vien-phuc-y-duong --phone 09xxxxxxxx --name "Chủ spa (thử)" --role owner
   unset APP_NEW_USER_PASSWORD
   ```
5. Bật backup: `sudo cp /opt/vpyd/infra/backup/vpyd-backup.{service,timer} /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable --now vpyd-backup.timer`, rồi chạy thử `sudo systemctl start vpyd-backup.service; journalctl -u vpyd-backup -n 30`.
6. **Smoke production** (S11 Tests): `curl -s https://staging.<domain>/api/v1/health` → `{"status":"ok",…}`; đăng nhập trên Chrome; `/bang-gia` hiện menu; console không có lỗi CSP; [securityheaders.com](https://securityheaders.com) đạt A trở lên; Data API ở §4.2 bước 1 không truy cập được.

**Rollback:** chạy workflow với `rollback = true`, hoặc trên VPS `infra/deploy.sh --rollback` (deploy lại tag trước trong `.deploy-history`). Migration phải theo kiểu expand/contract để API cũ vẫn chạy được trên schema mới.

## 7. Backup và diễn tập khôi phục
- **Backup đêm:** systemd timer 19:30 UTC (02:30 giờ VN) chạy `backup.sh`. Quy trình: `pg_dump -Fc` schema `app` + `public` (bảng `_sqlx_migrations`) → `age -r AGE_RECIPIENT` → `rclone rcat` lên `r2:vpyd-backups/staging/daily/…dump.age`. Dữ liệu được stream nên không có file plaintext nào nằm trên đĩa. Script kiểm tra kích thước object, xóa object nếu pipeline lỗi giữa chừng, và mùng 1 hằng tháng chép thêm một bản vào `monthly/`. Mật khẩu DB truyền qua biến libpq `PG*`, không nằm trong argv.
- **Diễn tập khôi phục (hằng tháng, và bắt buộc trước khi đóng S11):**
  ```bash
  cd /opt/vpyd; C="docker compose --env-file .env.production -f infra/docker-compose.prod.yml --profile drill"
  install -m 600 /dev/stdin secrets/age-identity.txt     # dán private key, Ctrl-D
  $C up -d --wait restore-db                              # Postgres 17 tạm, network internal, data trên tmpfs
  $C run --rm restore latest                              # tải + giải mã + pg_restore + in số dòng
  $C down -v; shred -u secrets/age-identity.txt
  ```
  Đạt khi: `restore.sh` báo `done`, `last_migration` khớp production, số dòng mỗi bảng khớp `SELECT count(*)` trên production (± các dòng mới sau giờ backup). Ghi kết quả (ngày, tag, thời gian khôi phục) vào `memory/log.md`.
- **Mất hẳn VPS:** tạo VPS mới → làm lại §3 → chép `.env.production` + CA từ password manager → chạy workflow deploy. DB không bị ảnh hưởng vì nằm ở Supabase. Mục tiêu dựng lại < 1 giờ (ADR-0005).
- **Mất DB:** dùng daily backup của Supabase trước, sau đó mới tới R2 qua `restore.sh` (đặt `RESTORE_PG*` trỏ vào DB mới; trên DB đó đã có role `app_rw`).

## 8. Uptime và cảnh báo (free tier)
- **Better Stack Uptime** (free: 10 monitor, kiểm tra mỗi 3 phút, 1 status page): monitor keyword `"status":"ok"` trên `https://staging.<domain>/api/v1/health`, báo qua email + app di động. [Better Stack vs UptimeRobot](https://betterstack.com/community/comparisons/better-stack-vs-uptimerobot/)
- **Healthchecks.io** (free Hobbyist: 20 check) làm dead-man's switch cho backup: tạo check "vpyd-staging-backup", lịch hằng ngày, grace 2 giờ, đặt URL vào `BACKUP_HEALTHCHECK_PING_URL`. Script ping `/start`, ping thành công hoặc `/fail`. [healthchecks.io FAQ](https://healthchecks.io/faq)
- UptimeRobot (free 50 monitor / 5 phút) cũng dùng được. Năm 2024 điều khoản từng cấm dùng free cho mục đích thương mại, các nguồn 2026 nói đã bỏ hạn chế này nhưng chưa xác minh được. [dev.to](https://dev.to/velprove/uptimerobot-commercial-use-free-alternatives-for-business-sites-in-2026-5d75)
- Log: `docker compose logs --since 1h api` (JSON, rotate 10 MB × 5). Chưa có log shipping (để sau R1).

## 9. EM cần làm gì, gửi lại gì

| # | Việc | Gửi lại cho agent (**không** phải secret) | Giữ riêng (secret, chỉ vào `.env.production` / password manager) |
|---|---|---|---|
| 1 | ✅ Tạo Supabase `vpyd-staging` (đã xong, ref `zyyiukcysjntjgjabiqe`, vùng Mỹ, Data API bật); rotate DB password; bật MFA. Lead bootstrap DB | Pooler host session mode (từ nút Connect) → vùng; Postgres major version | DB password `postgres`, mật khẩu `app_rw` |
| 2 | Mua VPS DigitalOcean Basic 2 GiB ($12) ở datacenter **khớp vùng Supabase** (§1.1: NYC3 hoặc SFO3), Ubuntu 24.04, gắn SSH key của EM | IP v4/v6; xác nhận đã làm §3 (hoặc cấp tạm cho agent một SSH key để làm) | Private SSH key |
| 3 | Mua domain, trỏ `A` `staging.<domain>` về VPS | Tên miền staging; email nhận thông báo ACME | — |
| 4 | Tạo R2 bucket `vpyd-backups` + lifecycle + bucket lock + token scope bucket | Account ID, tên bucket | Access Key ID + Secret |
| 5 | `age-keygen` trên máy EM | Public key `age1…` | Private key (offline) |
| 6 | Supabase §4.2: Exposed schemas không có `app` + REVOKE (bước 6), Network Restrictions theo IP VPS, Enforce SSL, tắt Auth signup | Xác nhận đã làm | — |
| 7 | GitHub environment `staging` + 3 secret SSH (§6.1) | Xác nhận đã tạo | Deploy private key |
| 8 | Tạo monitor Better Stack + check Healthchecks.io | Xác nhận | Ping URL (để thẳng vào `.env.production`) |

Sau đó agent chạy: deploy workflow → smoke → backup thử → diễn tập khôi phục → tick S11.

## 10. Rủi ro còn mở
- Staging ở Mỹ với Data API bật: an toàn nhờ REVOKE + RLS + chỉ dữ liệu giả; **không** nạp dữ liệu thật vào project này. Production dùng project Singapore mới, tắt Data API.
- Dữ liệu nằm ở nước ngoài (Singapore cho production): phải nộp đánh giá tác động chuyển dữ liệu ra nước ngoài trong vòng 60 ngày (Luật 91/2025 + NĐ 356/2025) **trước** khi nạp dữ liệu khách thật. Staging chỉ dùng dữ liệu giả.
- Một VPS là một điểm lỗi duy nhất (chấp nhận theo ADR-0005).
- `pg_dump` 17 trong image backup: nếu Supabase lên Postgres 18 thì phải đổi package trong `infra/backup/Dockerfile`.
- Backup dùng role owner `postgres` (owner thì bypass RLS, cần để dump đủ dữ liệu). Credential này chỉ nằm trong container `backup` và `migrate`, không bao giờ ở `api`.
