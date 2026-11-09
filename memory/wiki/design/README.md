# design/

Thiết kế UI/UX của app nội bộ, viết bởi skill `frontend-design`. Nguồn sự thật của code: `apps/web/src` (tokens ở `packages/ui/src/tokens/tokens.css`).

- [design-system.md](design-system.md): quy tắc (rulebook), tokens màu / chữ / khoảng cách / bóng, app shell + điều hướng, mobile patterns + API component dùng chung, class CSS dùng chung, motion, định dạng ngày/giờ/tiền, các đợt trước / sau.
- [front-desk-flows.md](front-desk-flows.md): luồng màn quầy `/quay` (ghi lượt, giỏ, bán gói, sửa / hủy) và tab Lịch hẹn, theo từng đợt EM review; phần đã thay được đánh dấu tại chỗ.

Prototype bấm thử (dữ liệu giả, không phải code chạy thật): `prototypes/front-desk/index.html`. Kiến trúc frontend (routes, data fetching, perf budget): [architecture/frontend.md](../architecture/frontend.md).
