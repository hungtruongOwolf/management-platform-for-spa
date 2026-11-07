import { ForbiddenRoute } from "./access";
import { BrandLogo } from "./BrandLogo";

/**
 * Whole-page fallbacks outside the shell (rulebook §9: say what happened and the next step).
 * 404 is its own lazy chunk; the error page is used only when a screen throws.
 */
export function StatusPage({ title, body, retry = false }: { title: string; body: string; retry?: boolean }) {
  return (
    <main id="main" className="status-page">
      <BrandLogo className="w-16" />
      <h1 className="page-title">{title}</h1>
      <p className="max-w-md text-text-muted">{body}</p>
      <div className="flex flex-wrap justify-center gap-2">
        {retry && (
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              window.location.reload();
            }}
          >
            Tải lại trang
          </button>
        )}
        <a href="/" className={`btn ${retry ? "btn-secondary" : "btn-primary"}`}>
          Về trang tổng quan
        </a>
      </div>
    </main>
  );
}

/** Inside the shell (its nav still works): the role may not open this screen. */
function ForbiddenPage() {
  return (
    <section aria-labelledby="forbidden-title" className="status-page is-in-shell">
      <BrandLogo className="w-16" />
      <h1 id="forbidden-title" className="page-title">
        Không có quyền mở màn hình này
      </h1>
      <p className="max-w-md text-text-muted">
        Tài khoản của bạn không dùng màn hình này. Các màn hình bạn dùng được nằm ở thanh bên; cần
        thêm quyền thì báo chủ spa.
      </p>
      <a href="/" className="btn btn-primary">
        Về trang tổng quan
      </a>
    </section>
  );
}

export function ErrorPage({ error }: { error?: unknown }) {
  if (error instanceof ForbiddenRoute) return <ForbiddenPage />;
  return (
    <StatusPage
      retry
      title="Màn hình này gặp lỗi"
      body="Dữ liệu đã lưu không bị mất. Tải lại trang để thử lần nữa; nếu vẫn lỗi, báo chủ spa kèm giờ xảy ra."
    />
  );
}

export function NotFoundPage() {
  return (
    <StatusPage
      title="Không tìm thấy trang này"
      body="Đường dẫn có thể đã cũ hoặc gõ nhầm. Về trang tổng quan, hoặc bấm Ctrl K ở đó để mở màn hình cần dùng."
    />
  );
}
