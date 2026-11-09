# Tính năng

Trang này liệt kê tính năng theo **vai trò** và theo **khu vực**, dùng đúng tên màn hình trong app (nav ở [`apps/web/src/app/nav.ts`](../apps/web/src/app/nav.ts), quy tắc đặt tên ở [glossary § Tên màn hình](../memory/wiki/product/glossary.md)). Yêu cầu và acceptance criteria đầy đủ nằm trong các spec ở [`memory/wiki/product/specs/`](../memory/wiki/product/specs/README.md); quyền chính xác của từng route ở [authz-matrix](../memory/wiki/security/authz-matrix.md).

Mọi giới hạn dưới đây được kiểm ở **server**. Nav và các nút chỉ ẩn những gì người dùng không làm được; mở thẳng URL thì gặp trang "Không có quyền", gọi thẳng API thì nhận `403`.

## 1. Theo vai trò

Vai trò là **tập permission lưu trong DB** (`role_permission`), không phải `if role == …` trong code. Ba vai trò hệ thống có mẫu quyền mặc định; operator nền tảng chỉnh được mẫu và quyền riêng của từng spa (có rào chắn, mục 4).

| Vai trò | Ai | Làm được gì (mặc định) |
|---|---|---|
| **Chủ spa** (`owner`) | Chủ cơ sở | Mọi quyền trong spa của mình. Bắt buộc MFA (TOTP). Riêng chủ spa: sửa bảng giá và mẫu gói, giảm giá không giới hạn, gia hạn gói, hủy gói + hoàn tiền, hủy bản ghi của ngày đã qua, nhập bù lượt của ngày trước, cài đặt lịch hẹn (số khách tối đa), xem và chốt bảng lương, quản lý hồ sơ nhân viên + tạo tài khoản đăng nhập + đặt lại mật khẩu, báo cáo dài hạn 3–5 năm |
| **Quản lý** (`manager`) | Quản lý kiêm thu ngân | Quầy: ghi lượt, bán gói, thu tiền, giảm giá trong trần của spa (`manager_discount_max_pct`), hủy bản ghi **trong ngày**, dùng gói đã hết hạn kèm lý do. Lịch hẹn. Khách hàng + ghi chú sức khỏe + gọi lại khách vắng. Báo cáo (kể cả tiền), Nhật ký ghi chú. Chấm công, khen/phàn nàn, đánh giá tháng, thưởng/phạt cho **KTV** (không ghi cho chính mình hay quản lý khác); xem danh sách nhân sự. **Không** thấy lương. Hồ sơ của chính mình nằm ở "Công và lương của tôi", không ở sổ quản lý |
| **Kỹ thuật viên** (`technician`) | Người làm dịch vụ | Xem bảng giá; "Công và lương của tôi" (ngày công, đánh giá, phiếu lương của mình); "Khách của tôi" (lịch hẹn gán cho mình và lượt mình đã làm, kèm ghi chú buổi của lượt đó; không SĐT, không tiền) |
| **Người đã nghỉ** (suy ra, không cấp được) | Nhân viên có hồ sơ đã nghỉ việc, tài khoản chưa bị khóa | Chỉ `staff.read_own_past`: công và phiếu lương của **các tháng đã làm**. Quyền này do server suy ra mỗi request từ `staff.status = 'left'`; chủ spa không bao giờ bị hạ ([ADR-0014](../memory/wiki/decisions/ADR-0014-former-staff-login.md)) |
| **Operator nền tảng** | Người vận hành nền tảng bán cho nhiều spa | Console `/admin` riêng: đăng nhập riêng, MFA luôn bắt buộc, xác thực lại (step-up) trước mỗi thao tác ghi. Thấy số đếm và metadata của spa, **không** thấy dữ liệu khách, lượt hay lương ([ADR-0018](../memory/wiki/decisions/ADR-0018-platform-admin-console.md)) |

### Màn hình theo vai trò

| Nhóm nav | Màn hình (đường dẫn) | Chủ spa | Quản lý | KTV |
|---|---|:-:|:-:|:-:|
| Quầy | Tổng quan (`/`) | ✓ | ✓ | ✓ (lối tắt) |
| Quầy | Ghi lượt khách (`/quay`) | ✓ | ✓ | |
| Quầy | Lịch hẹn (`/quay?view=lich`) | ✓ | ✓ | |
| Khách hàng | Danh sách khách hàng (`/khach-hang`, hồ sơ `/khach-hang/$id`) | ✓ | ✓ | |
| Khách hàng | Khách lâu chưa quay lại (`/khach-lau-khong-quay-lai`) | ✓ | ✓ | |
| Gói liệu trình | Gói liệu trình của khách (`/goi-lieu-trinh`) | ✓ | ✓ | |
| Báo cáo | Báo cáo kinh doanh (`/bao-cao`, tab Tổng quan · Sổ doanh thu) | ✓ | ✓ | |
| Báo cáo | Nhật ký ghi chú (`/nhat-ky-ghi-chu`) | ✓ | ✓ | |
| Nhân viên | Quản lý nhân sự (`/nhan-vien`) | ✓ | ✓ (không tab Bảng lương) | |
| Nhân viên | Công và lương của tôi (`/nhan-vien/cua-toi`; KTV mở ở `/nhan-vien`) | | ✓ | ✓ |
| Nhân viên | Khách của tôi (`/khach-cua-toi`) | | ✓ | ✓ |
| Danh mục | Bảng giá dịch vụ (`/bang-gia`) | ✓ sửa | ✓ xem | ✓ xem |
| Danh mục | Mẫu gói liệu trình (`/bang-gia/goi`) | ✓ sửa | ✓ xem | ✓ xem |
| Cài đặt | Giao diện & tài khoản (`/cai-dat`) | ✓ | ✓ | ✓ |

Trên điện thoại, tab bar hiện tối đa 4 màn hay dùng nhất (khi có Lịch hẹn thì Lịch hẹn thay Trang chủ); còn lại nằm trong ngăn "Thêm".

## 2. Theo khu vực

### 2.1 Đăng nhập và tài khoản
- **Đăng nhập** (`/dang-nhap`) bằng SĐT + mật khẩu. Ai cần MFA thì qua **Xác thực hai bước** (`/xac-thuc`): lần đầu quét QR (hoặc nhập khóa tay) và nhận mã khôi phục dùng một lần, các lần sau nhập mã 6 số.
- **Mật khẩu tạm**: tài khoản do chủ spa tạo hoặc vừa được đặt lại phải **Đổi mật khẩu** (`/doi-mat-khau`) trước khi dùng bất cứ màn nào ([ADR-0016](../memory/wiki/decisions/ADR-0016-accounts-and-passwords.md)).
- **"Quên mật khẩu?"** gửi yêu cầu tới chủ spa (không email/SMS); chủ spa thấy yêu cầu ở Tổng quan và đặt lại trong app. Yêu cầu hết hạn sau 24 giờ.
- **Giao diện & tài khoản** (`/cai-dat`): giao diện sáng/tối, tài khoản đang đăng nhập, đổi mật khẩu.
- Spa đang bị operator tạm ngưng: đăng nhập đúng mật khẩu vẫn bị chặn với thông báo "spa đang tạm ngưng".

### 2.2 Quầy: Tổng quan và Ghi lượt khách
Spec: [front-desk-and-packages](../memory/wiki/product/specs/front-desk-and-packages.md), luồng màn: [front-desk-flows](../memory/wiki/design/front-desk-flows.md).
- **Tổng quan** (`/`): khối "Hôm nay" (thực thu nếu có quyền xem tiền, số khách, số buổi liệu trình đã trừ…) và lối tắt tới từng màn có quyền.
- **Ghi lượt khách** (`/quay`, tab "Ghi lượt"): đường nhanh tìm khách → "Trừ 1 buổi" → chọn KTV → Lưu (mục tiêu < 30 giây, ≤ 4 thao tác).
  - Tìm khách theo tên không dấu hoặc SĐT; khách vãng lai không cho tên / SĐT vẫn ghi được (hồ sơ "Khách vãng lai").
  - Một lượt gồm dòng **trừ buổi gói** và/hoặc **dịch vụ lẻ** (giá Nữ/Nam theo giới tính), KTV từng dòng, người giới thiệu (upsell) cho dòng lẻ, **giảm giá** có lý do, thu tiền mặt / chuyển khoản / cả hai.
  - **Giỏ bán gói**: bán tối đa 3 gói trong một lần lưu (mỗi gói một hóa đơn, một transaction), "Làm 1 buổi hôm nay" trừ buổi ngay từ gói vừa bán; mua cho người khác (người trả ≠ người dùng gói); giỏ nháp giữ khi rời màn.
  - **Người đi cùng**: ghi chú tên, hoặc người được làm dịch vụ riêng (lịch riêng trong nhóm, [ADR-0009](../memory/wiki/decisions/ADR-0009-served-companions.md)).
  - **Giờ làm** (mặc định bây giờ) và **nhập bù** ngày đã qua (chỉ chủ spa); đồng hồ đếm ngược thời lượng dịch vụ.
  - **Lượt hôm nay**: danh sách mở inline; **Sửa lượt** (đổi KTV / giờ / ghi chú trực tiếp; đụng tiền hoặc buổi thì hủy + ghi lại, hiện chênh lệch "Thu thêm" / "Hoàn"), **Hủy lượt** (buổi cộng lại, tiền ghi bút toán đảo), **Thêm dịch vụ** vào lượt đã lưu, **Chuyển sang gói liệu trình** (lượt lẻ đã trả được thay bằng lượt trừ buổi của gói vừa mua, khách đưa thêm phần chênh).
  - Hóa đơn bán gói không kèm lượt của ngày cũng hiện ở Quầy, hủy được trong ngày.
  - **Ghi chú buổi** và **ghi chú gói** là dữ liệu sức khỏe: mã hóa, chỉ người có quyền ghi chú sức khỏe mới đọc/ghi.

### 2.3 Lịch hẹn
Spec: [appointments](../memory/wiki/product/specs/appointments.md).
- Tab **Lịch hẹn** của `/quay`: xem theo ngày (lưới giờ, thẻ đúng số phút), tuần, danh sách, hoặc theo KTV; lượt vãng lai cũng hiện trên dòng thời gian (Chưa đến / Đang làm / Xong).
- **Đặt lịch** cho khách có hồ sơ hoặc khách mới (tên + SĐT); **đặt nhóm / gia đình** (mỗi người một lịch, một KTV, một lượt).
- Trạng thái: Đã đặt → Đã đến (có "Hoàn tác đã đến") → Hoàn tất (thành lượt khách qua đúng luồng Quầy); hoặc Không đến / Hủy lịch kèm lý do.
- Server chặn: một KTV hai lịch chồng giờ (`EXCLUDE` ở DB), một khách hai lịch chồng giờ, đặt vào giờ đã qua. "Đổi cả nhóm" dời mọi người hoặc không ai.
- Chủ spa đặt **số khách tối đa** mỗi khung giờ (chỉ cảnh báo).

### 2.4 Khách hàng
Spec: [management-views](../memory/wiki/product/specs/management-views.md), [front-desk-and-packages](../memory/wiki/product/specs/front-desk-and-packages.md).
- **Danh sách khách hàng** (`/khach-hang`): gần đây + tìm không dấu / theo SĐT, kiểm trùng SĐT khi tạo.
- **Hồ sơ khách** (`/khach-hang/$id`): thông tin (SĐT và tên tùy chọn, ngày sinh, giới tính), ghi chú, **ghi chú sức khỏe** (mã hóa), gói liệu trình của khách, lịch sử (lượt, hóa đơn, gia hạn, hủy… lọc theo loại và ngày), tóm tắt.
- **Khách lâu chưa quay lại** (`/khach-lau-khong-quay-lai`): khách vắng theo số ngày, ghi lần liên lạc (kênh, kết quả, ngày hẹn gọi lại) — chỉ thêm, không sửa.

### 2.5 Gói liệu trình
- **Gói liệu trình của khách** (`/goi-lieu-trinh`): mọi gói theo trạng thái (đang dùng, sắp hết, đã hết buổi, hết hạn, đã hủy), tìm và lọc.
- Vòng đời gói: **gia hạn** có lý do (chủ spa, ghi vào sổ gia hạn và dòng thời gian khách); **dùng gói đã hết hạn** với lý do (quản lý, chủ spa); **hủy + hoàn tiền** phần chưa dùng (chủ spa, một lần, không vượt giá bán). Trạng thái gói luôn suy ra từ dữ liệu, không ai phải cập nhật tay.
- Gói ghi trên **sổ giấy cũ** được nhập một lần bằng CLI `app import-paper-book` (việc của đội, không phải màn quầy); tiền sổ giấy thành hóa đơn thật trong báo cáo.

### 2.6 Báo cáo
Spec: [management-views](../memory/wiki/product/specs/management-views.md).
- **Báo cáo kinh doanh** (`/bao-cao`), tab **Tổng quan**: chọn kỳ (preset hoặc tự chọn, nằm trên URL), số chính so với kỳ trước cùng độ dài, biểu đồ xu hướng, chia theo ngày / tuần / tháng, theo KTV, dịch vụ / gói bán chạy; tiền mặt / chuyển khoản, bán gói / dịch vụ lẻ. Tối đa 2 năm; chủ spa xem tới 5 năm.
- Tab **Sổ doanh thu**: sổ thu như sổ giấy (mỗi hóa đơn một dòng, dòng âm cho hủy và hoàn).
- Số tiền chỉ có trong response khi người xem có `report.revenue.read`; chỉ có `report.read` thì thấy số đếm.
- **Nhật ký ghi chú** (`/nhat-ky-ghi-chu`): mọi ghi chú viết trong app (16 loại: ghi chú lượt, hồ sơ khách, chấm công, khen/phàn nàn, thưởng/phạt, gọi lại, lý do hủy, lịch hẹn, đánh giá tháng, ghi chú nhân sự, gia hạn, giảm giá, lý do hủy lịch / không đến, ghi chú gói…), mới nhất trước, gom theo ngày, link về nguồn; mỗi loại giữ quyền đọc của màn gốc.
- Chưa có: sổ thu chi / phiếu chi / chốt quỹ (để sau).

### 2.7 Nhân viên
Spec: [staff-management](../memory/wiki/product/specs/staff-management.md), quy chế: [compensation-rules](../memory/wiki/product/compensation-rules.md).
- **Quản lý nhân sự** (`/nhan-vien`), các tab:
  - **Sổ hằng ngày**: mỗi người một thẻ, chấm công (Đi làm / Nghỉ có phép / Nghỉ đột xuất / Nghỉ không phép, giờ đến / về, phút muộn do server tính), khen / phàn nàn, thưởng / phạt; cảnh báo khi chấm nghỉ cho người đã có lượt khách trong ngày.
  - **Bảng công tháng**: lưới cả tháng.
  - **Đánh giá tháng**: 6 tiêu chí, 100 điểm → Mức 1 / 2 / 3 / Chưa đạt (mức thấp bắt buộc kế hoạch kèm cặp); nhắc khi đến hạn; xem lại phiếu cũ và lịch sử 12 tháng.
  - **Bảng lương** (chỉ chủ spa): tính từ công, đánh giá, hoa hồng (theo tiền giữ lại thật, [ADR-0010](../memory/wiki/decisions/ADR-0010-commission-follows-kept-money.md)), thưởng / phạt; **chốt tháng** tạo phiếu lương bản chụp và khóa sổ của tháng đó, không mở lại ([ADR-0008](../memory/wiki/decisions/ADR-0008-payroll-close-snapshots.md)).
  - **Hồ sơ nhân viên**: lọc Đang làm / Đã nghỉ / Tất cả, thâm niên theo các đợt làm việc; chủ spa thêm / sửa hồ sơ, **nghỉ việc** có lý do (liệt kê lịch hẹn còn vướng, tùy chọn khóa tài khoản), **nhận lại**, **tạo tài khoản đăng nhập** từ hồ sơ (vai trò + mật khẩu tạm), đặt lại mật khẩu, đổi vai trò, lịch sử thay đổi.
- **Công và lương của tôi**: tổng quan tháng, lịch công, chi tiết một ngày, phiếu lương nhóm theo loại, đánh giá + lịch sử 12 tháng — của chính người đăng nhập (người lấy từ session).
- **Khách của tôi**: lịch kiểu Google Calendar (ngày / tuần / danh sách) gồm lịch hẹn gán cho mình và lượt mình đã làm, chỉ đọc.

### 2.8 Danh mục
- **Bảng giá dịch vụ** (`/bang-gia`): dịch vụ theo nhóm, giá chung hoặc Nữ / Nam, thời lượng, dịch vụ gợi ý; menu thật của spa có 35 dịch vụ trong 10 nhóm ([service-catalog](../memory/wiki/product/service-catalog.md)).
- **Mẫu gói liệu trình** (`/bang-gia/goi`): dịch vụ + số buổi + buổi tặng theo từng dịch vụ, giá, quà tặng, hạn dùng.

## 3. Cờ tính năng theo spa

Operator bật / tắt từng tính năng cho từng spa ở console. Không có dòng nào trong `tenant_feature` = bật. Khi tắt, server rút các permission của tính năng khỏi mọi thành viên spa và các route của nó trả `403 feature.disabled`; nav tự ẩn mục tương ứng. Registry nằm ở [`kernel/src/feature.rs`](../apps/api/crates/kernel/src/feature.rs).

| Key | Tên trong console | Khi tắt |
|---|---|---|
| `appointments` | Lịch hẹn | Mất tab Lịch hẹn, cài đặt lịch hẹn, lịch trong "Khách của tôi" |
| `payroll` | Bảng lương | Mất tab Bảng lương và phiếu lương |
| `evaluations` | Đánh giá tháng | Mất đánh giá tháng (khen/phàn nàn và thưởng/phạt hằng ngày vẫn còn) |
| `follow_ups` | Gọi lại khách vắng | Mất việc ghi lần liên lạc với khách vắng |
| `notes_journal` | Nhật ký ghi chú | Mất màn Nhật ký ghi chú |
| `reports` | Báo cáo kinh doanh | Mất báo cáo theo kỳ, sổ doanh thu, báo cáo dài hạn |

## 4. Console nền tảng (`/admin`)

Spec: [platform-admin](../memory/wiki/product/specs/platform-admin.md), thiết kế: [platform-admin](../memory/wiki/architecture/platform-admin.md). Console là ứng dụng con riêng, nạp lazy, không ảnh hưởng bundle của spa. Mọi thao tác ghi cần **xác thực lại trong 5 phút** (mật khẩu + mã 6 số), **lý do** bắt buộc, và được ghi vào nhật ký nền tảng (cùng nhật ký của spa bị tác động).

| Màn | Đường dẫn | Việc |
|---|---|---|
| Đăng nhập | `/admin/dang-nhap` | Username + mật khẩu → MFA (lần đầu enroll, nhận mã khôi phục một lần) |
| Spa | `/admin` | Danh sách spa: trạng thái, ngày tạo, số nhân viên, số khách, lượt 30 ngày, hoạt động gần nhất (chỉ số đếm). **Tạo spa** + chủ spa đầu tiên (mật khẩu tạm hiện một lần) |
| Một spa | `/admin/spa/$id` | Panel **Thông tin** + Tạm ngưng spa / Mở lại spa (tạm ngưng thu hồi mọi phiên của spa); **Tính năng** (công tắc + lý do); **Vai trò của spa** (Theo mẫu / Riêng spa này / Về mẫu); **Nhật ký của spa** (chỉ metadata: lúc, hành động, loại, ai) |
| Mẫu vai trò | `/admin/vai-tro` | Sửa quyền mặc định của Chủ spa / Quản lý / KTV; áp ngay cho mọi spa đang theo mẫu. Rào chắn: chỉ owner có `staff.manage`, owner luôn giữ các quyền tối thiểu, không cấp được quyền nền tảng hay quyền suy ra |
| Tài khoản | `/admin/tai-khoan` | Tìm theo SĐT / tên (≥ 3 ký tự, ≤ 50 kết quả), xem spa của từng người; Khóa / Mở khóa (không khóa chủ spa cuối cùng của spa đang hoạt động), Đặt lại mật khẩu, Đặt lại xác thực hai bước, Đăng xuất mọi thiết bị |
| Nhật ký | `/admin/nhat-ky` | Nhật ký nền tảng: mọi thao tác của operator |

Tạo operator đầu tiên và xử lý mất authenticator bằng CLI (`app add-operator`, `app reset-operator-mfa`, `app disable-operator` / `enable-operator`); xem [platform-admin § Vận hành](../memory/wiki/architecture/platform-admin.md).

## 5. Chưa làm / để sau
Theo [vision](../memory/wiki/product/vision.md) và các spec: sổ thu chi và chốt quỹ cuối ngày, công nợ / trả một phần, ẩn danh hồ sơ khách (permission `customer.anonymize` và cột `anonymized_at` đã có, chưa có route), nhắc nhở Zalo / SMS, đặt lịch online, trang giới thiệu, quản lý nhiều chi nhánh, bán sản phẩm và tồn kho, native app. Câu hỏi mở của EM nằm ở [overview](../memory/wiki/overview.md) và cuối mỗi spec.
