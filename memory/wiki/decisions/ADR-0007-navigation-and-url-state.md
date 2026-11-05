---
title: ADR-0007 Navigation model and URL list state (internal app)
status: accepted
date: 2026-11-05
deciders: EM (feedback), agent
---

# ADR-0007: Navigation model and URL list state

## Context
EM vòng 3 ([raw](../../raw/2026-11-05-em-ui-review-round3.md)): từ danh sách bấm vào chi tiết, chuyển sang trang khác, rồi "muốn quay về xem thì không biết làm thế nào"; yêu cầu nghiên cứu cho "chuẩn chỉ". Trước đó mỗi màn tự làm tiêu đề, hồ sơ khách có link cố định "‹ Danh sách khách", bộ lọc nằm trong `useState` (mất khi rời trang), router không khôi phục vị trí cuộn.

Mẫu đã xem:
- **GOV.UK Design System:** breadcrumb cho vị trí trong cây, link "Back" riêng cho hành trình; không dùng cả hai cho cùng mục đích; breadcrumb có thể ẩn trên điện thoại. https://design-system.service.gov.uk/components/breadcrumbs/ , https://design-system.service.gov.uk/components/back-link/
- **Shopify Polaris (Page):** trang chi tiết có `backAction` (mũi tên + nhãn) cạnh tiêu đề; section trên nav vẫn sáng khi ở trang con. https://polaris.shopify.com/components/layout-and-structure/page
- **Material Design 3 (top app bar / "Up"):** Up = về cha trong cây, Back = lịch sử; trên web hai thứ thường trùng. https://m3.material.io/components/top-app-bar/guidelines
- **Apple HIG (navigation bars):** nút back mang **tên màn trước**, để người dùng biết mình sẽ về đâu. https://developer.apple.com/design/human-interface-guidelines/navigation-bars
- **TanStack Router:** search params là state hạng nhất (`validateSearch`, typed `Link search`), `scrollRestoration`, `history.state.__TSR_index`. https://tanstack.com/router/latest/docs/framework/react/guide/search-params

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Chỉ breadcrumb theo cây | đơn giản, luôn đúng cha | mất bộ lọc / vị trí; mở từ "Theo dõi gói" mà về "Khách hàng" là sai hành trình |
| B. Chỉ `history.back()` | giữ nguyên trạng thái | deep link / tab mới không có lịch sử → nút chết; không biết sẽ về đâu |
| C. **Breadcrumb theo cây + nút quay lại theo lịch sử có tên đích + fallback về cha; state danh sách trên URL** | đúng cả hai câu hỏi "tôi ở đâu" và "tôi từ đâu tới"; reload/chia sẻ link giữ nguyên danh sách | thêm một module trail nhỏ (sessionStorage); mỗi màn danh sách phải khai báo schema |
| D. Global store (Zustand/Redux) giữ state danh sách | không động URL | mất khi reload, không chia sẻ được, trùng với cache Query, thêm dependency trên critical path |

## Decision
Chọn **C**:
- `PageHeader` cho mọi màn: breadcrumb (≥ 640px) = cây; nút quay lại = entry lịch sử trước đó trong app (nhãn là tiêu đề màn đó, `history.back()`), không có thì về crumb cha. Khi đích quay lại là chính cha, chỉ hiện một thứ.
- `app/trail.ts` ghi URL + tiêu đề theo `__TSR_index`, sessionStorage mỗi tab; crumb tới danh sách mở lại URL cuối của danh sách đó.
- Trạng thái danh sách (tìm, lọc, tab, ngày, trang) chỉ nằm trên URL qua `searchSchema` + `useSearchState` (`lib/searchState.ts`); lọc dùng `replace`; mặc định không lên URL; parser viết tay (không zod: router ở critical path).
- Sidebar sáng theo URL (section của trang chi tiết). Router `scrollRestoration: true`.

## Consequences
- Positive: Back, reload, link chia sẻ đều mở lại đúng danh sách; mọi trang chi tiết có đường về có tên; feature mới chỉ khai báo schema + dùng `PageHeader`.
- Negative / accepted trade-offs: ~1.7 KB gzip trên bundle khởi động (trail + schema + PageHeader + menu tài khoản; 117.5 → 119.2 KB, ngân sách 125). Trail mất khi đóng tab (cố ý). Danh sách "Xem thêm" (cursor) không đưa cursor lên URL: quay lại vẫn còn các trang đã tải nhờ cache Query (gcTime 5 phút), reload thì về trang đầu.
- Follow-ups: features agent thêm `q` cho "Theo dõi gói", khoảng ngày cho `/bao-cao` vào schema có sẵn; khi có trang liệu trình `/khach-hang/:id/goi/:pkg` thì breadcrumb `Khách hàng › Tên › Gói`.

## Revisit when
Có app native (React Native) dùng chung điều hướng, hoặc cần trạng thái danh sách đồng bộ giữa nhiều tab/thiết bị.
