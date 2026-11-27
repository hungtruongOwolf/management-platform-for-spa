/**
 * Short names of the screens, for a back control whose destination has no PageHeader title (the
 * front desk, home). Paths not listed fall back to the nearest listed parent.
 *
 * Its own module, outside nav.ts: only the page header (a lazy chunk) reads it, so the table stays
 * out of the startup chunk (frontend.md "Perf budget").
 */
const SCREEN_LABELS: Record<string, string> = {
  "/": "Tổng quan",
  "/quay": "Quầy",
  "/khach-hang": "Khách hàng",
  "/khach-lau-khong-quay-lai": "Khách lâu chưa quay lại",
  "/vong-quay": "Vòng quay may mắn",
  "/goi-lieu-trinh": "Gói liệu trình của khách",
  "/bao-cao": "Báo cáo kinh doanh",
  "/bang-gia": "Bảng giá dịch vụ",
  "/bang-gia/goi": "Mẫu gói liệu trình",
  "/nhan-vien": "Quản lý nhân sự",
  "/nhan-vien/cua-toi": "Công và lương của tôi",
  "/khach-cua-toi": "Khách của tôi",
  "/cai-dat": "Cài đặt",
};

export function screenLabel(pathname: string): string {
  for (
    let p = pathname;
    p.length > 0;
    p = p.slice(0, Math.max(0, p.lastIndexOf("/")))
  ) {
    const label = SCREEN_LABELS[p];
    if (label) return label;
  }
  return "Tổng quan";
}
