import { useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { PageHeader } from "../../app/PageHeader";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { customerQuery, recentQuery } from "./api";
import { CustomerForm } from "./CustomerForm";
import { CustomerProfile } from "./CustomerProfile";
import { CustomerSearch } from "./CustomerSearch";
import { customersSearch } from "./search";
import { CustomerHistory } from "../reports/CustomerHistory";
import { ErrorState } from "../reports/ui";
import "./khach.css";

/** The customers list, parent of every profile (breadcrumbs, back after a deep link). */
const CUSTOMERS: { label: string; to: string } = { label: "Khách hàng", to: "/khach-hang" };

/** /khach-hang — find or register a customer. */
export function CustomersPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const navigate = useNavigate();
  const [creating, setCreating] = useState<string | null>(null);
  const [search, setSearch] = useSearchState(customersSearch);
  const recent = useQuery(recentQuery);
  const open = (id: string) =>
    void navigate({
      to: "/khach-hang/$customerId",
      params: { customerId: id },
    });

  return (
    <section aria-labelledby="customers-title" className="khach-page">
      <PageHeader title="Danh sách khách hàng" titleId="customers-title" />
      <div className="panel">
        <CustomerSearch
          query={search.q}
          onQueryChange={(q) => {
            setSearch({ q });
          }}
          onSelect={(c) => {
            open(c.id);
          }}
          onCreate={(typed) => {
            setCreating(typed);
          }}
        />
        {creating !== null && can(me, "customer.write") && (
          <div className="panel-section">
            <h2 className="panel-title">Khách mới</h2>
            <CustomerForm
              typed={creating}
              canWriteHealth={can(me, "customer.health_notes.write")}
              onSaved={(c) => {
                setCreating(null);
                open(c.id);
              }}
              onOpenExisting={open}
              onCancel={() => {
                setCreating(null);
              }}
            />
          </div>
        )}
      </div>
      <section aria-labelledby="recent-title" className="panel">
        <h2 id="recent-title" className="panel-title">
          Khách gần đây
        </h2>
        {recent.isError && <ErrorState error={recent.error} onRetry={() => void recent.refetch()} />}
        {recent.isPending && (
          <div role="status" aria-label="Đang tải khách gần đây" className="grid gap-3">
            {[0, 1, 2, 3].map((i) => (
              <div key={i} className="skeleton h-12" />
            ))}
          </div>
        )}
        {recent.data?.length === 0 && (
          <p className="khach-empty">Chưa có khách nào. Gõ tên hoặc số điện thoại vào ô tìm ở trên rồi bấm “Khách mới” để tạo hồ sơ đầu tiên.</p>
        )}
        {recent.data && recent.data.length > 0 && (
          <ul className="row-list">
            {recent.data.map((c) => (
              <li key={c.id}>
                <Link to="/khach-hang/$customerId" params={{ customerId: c.id }} className="row-link">
                  <span className="flex min-w-0 items-center gap-3">
                    {/* Given name's initial (Vietnamese names end with it), decorative. */}
                    <span aria-hidden="true" className="avatar">
                      {c.full_name.trim().split(/\s+/).at(-1)?.charAt(0).toUpperCase()}
                    </span>
                    {/* Phones: the number goes under the name, so a long name keeps its chip on one line. */}
                    <span className="grid min-w-0">
                      <CustomerName name={c.full_name} gender={c.gender} className="font-semibold" />
                      <span className="row-meta tabular sm:hidden">{c.phone}</span>
                    </span>
                  </span>
                  <span className="row-meta shrink-0 whitespace-nowrap tabular max-sm:hidden">{c.phone}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>
    </section>
  );
}

/** Route component for /khach-hang/:customerId (loaded lazily by the router). */
export function CustomerRoutePage() {
  const { customerId } = useParams({ from: "/authed/khach-hang/$customerId" });
  return <CustomerPage customerId={customerId} />;
}

/** /khach-hang/:customerId — profile with edit, and the usage history beside it. */
export function CustomerPage({ customerId }: { customerId: string }) {
  const queryClient = useQueryClient();
  const { data: me } = useSuspenseQuery(meQuery);
  const customer = useQuery(customerQuery(customerId));
  // Loading and errors keep the header, so the way back is always there.
  if (customer.isPending)
    return (
      <div className="khach-page">
        <PageHeader breadcrumbs={[CUSTOMERS]} title={<span className="skeleton inline-block h-8 w-56 align-middle" />} />
        <div role="status" aria-label="Đang tải hồ sơ…" className="khach-cols">
          <div className="skeleton h-64" />
          <div className="skeleton h-64" />
        </div>
      </div>
    );
  if (customer.isError) {
    return (
      <div className="khach-page">
        <PageHeader breadcrumbs={[CUSTOMERS]} title="Hồ sơ khách hàng" />
        <ErrorState error={customer.error} onRetry={() => void customer.refetch()} />
      </div>
    );
  }
  const c = customer.data;
  return (
    <div className="khach-page">
      <PageHeader
        breadcrumbs={[CUSTOMERS]}
        title={<CustomerName name={c.full_name} gender={c.gender} showUnknown />}
        label={c.full_name}
        titleId="customer-name"
      />
      <div className="khach-cols">
        <CustomerProfile
          customer={customer.data}
          onChanged={(c) => {
            queryClient.setQueryData(customerQuery(customerId).queryKey, c);
          }}
        />
        <CustomerHistory customerId={customerId} customerName={c.full_name} me={me} />
      </div>
    </div>
  );
}
