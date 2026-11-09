# Tài liệu

Điểm vào cho người mới đọc hệ thống. Ba trang dưới đây tóm tắt ở mức tổng quan nhưng đủ chi tiết để bắt tay vào làm, và link sâu vào wiki của dự án.

| Trang | Trả lời câu hỏi |
|---|---|
| [system-design.md](system-design.md) | Hệ thống gồm những thành phần nào? Một request đi qua những bước nào (cookie → `session_actor` → permission → `TenantTx`/RLS → handler → audit)? Ranh giới tin cậy và mô hình bảo mật? Chạy ở đâu (Vercel pdx1, Supabase us-west-2)? Job nền nào chạy? Ngân sách hiệu năng? |
| [database-design.md](database-design.md) | Các nhóm bảng và quan hệ (ER), RLS và hàm `SECURITY DEFINER`, ràng buộc toàn vẹn (CHECK tiền, chỉ thêm, `EXCLUDE`, FK, `version`, idempotency), cột mã hóa, index và kết quả rà soát 2026-11, migration theo dải số |
| [features.md](features.md) | Ai làm được gì (chủ spa, quản lý, KTV, người đã nghỉ, operator), từng màn hình theo tên trong app, cờ tính năng theo spa, console nền tảng `/admin` |

## Quan hệ với `memory/`

- [`memory/`](../memory/README.md) là **nguồn sự thật**: wiki do agent duy trì (mô hình LLM Wiki), gồm spec, ADR, thiết kế, runbook, nhật ký. Bắt đầu từ [`memory/index.md`](../memory/index.md).
- `docs/` là **bản đọc nhanh**: không thay wiki, không chép lại chi tiết. Khi một quyết định đổi, sửa wiki trước (ADR, trang kiến trúc), rồi sửa câu tóm tắt ở đây nếu nó còn sai.
- `memory/raw/` chứa nguồn gốc bất biến (yêu cầu nguyên văn của EM, ghi chú họp). Không sửa file trong đó.

## Đọc tiếp theo mục đích

| Bạn muốn | Đọc |
|---|---|
| Hiểu sản phẩm và người dùng | [vision](../memory/wiki/product/vision.md), [personas](../memory/wiki/product/personas.md), [paper-books-analysis](../memory/wiki/product/paper-books-analysis.md), [glossary](../memory/wiki/product/glossary.md) |
| Yêu cầu chi tiết của một tính năng | [specs/](../memory/wiki/product/specs/README.md) |
| Vì sao chọn X | ADR trong [decisions/](../memory/index.md) (ADR-0001 … ADR-0018) |
| Viết code đúng chuẩn | [coding-standards](../memory/wiki/conventions/coding-standards.md), [testing](../memory/wiki/conventions/testing.md), [`CLAUDE.md`](../CLAUDE.md) |
| Quyền của một route | [authz-matrix](../memory/wiki/security/authz-matrix.md), [threat-model](../memory/wiki/security/threat-model.md) |
| Giao diện, màu, component | [design-system](../memory/wiki/design/design-system.md), [front-desk-flows](../memory/wiki/design/front-desk-flows.md) |
| Deploy, staging, merge khi CI tắt | [vercel-staging](../memory/wiki/runbooks/vercel-staging.md), [local-merge-gate](../memory/wiki/runbooks/local-merge-gate.md) |

## Tài liệu nghiệp vụ gốc

Thư mục [`docs for reference/`](../docs%20for%20reference) giữ tài liệu thật của spa mà sản phẩm dựa vào: bảng chấm công và đánh giá tháng (xlsx), menu dịch vụ hai mặt (png), chương trình tuyển dụng KTV và đào tạo học viên (docx), ảnh các sổ giấy đang dùng (doanh thu hằng ngày, liệu trình khách hàng, thu chi) trong `sheet cho quản lý khách hàng/`, logo. Đây là input cho spec ([compensation-rules](../memory/wiki/product/compensation-rules.md), [service-catalog](../memory/wiki/product/service-catalog.md)), không phải tài liệu kỹ thuật.
