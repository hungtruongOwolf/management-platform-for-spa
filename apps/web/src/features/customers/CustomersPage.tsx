import {
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { PageHeader } from "../../app/PageHeader";
import { messageFor } from "../../lib/errors";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { customerQuery, recentQuery } from "./api";
import { CustomerForm } from "./CustomerForm";
import { CustomerProfile } from "./CustomerProfile";
import { CustomerSearch } from "./CustomerSearch";
import { customersSearch } from "./search";
import { CustomerHistory } from "../reports/CustomerHistory";

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
    <section aria-labelledby="customers-title" className="grid content-start gap-4">
      <PageHeader eyebrow="Hồ sơ khách" title="Khách hàng" titleId="customers-title" />
      <div className="card p-4">
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
          <div className="mt-3">
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
      <section aria-labelledby="recent-title">
        <h2 id="recent-title" className="mb-2 font-semibold">
          Khách gần đây
        </h2>
        {recent.isError && (
          <p role="alert" className="text-danger">
            {messageFor(recent.error)}
          </p>
        )}
        {recent.isPending && (
          <div role="status" aria-label="Đang tải khách gần đây" className="card grid content-start gap-2 p-3">
            {[0, 1, 2, 3].map((i) => (
              <div key={i} className="skeleton h-12" />
            ))}
          </div>
        )}
        {recent.data?.length === 0 && (
          <p className="text-text-muted">Chưa có khách nào.</p>
        )}
        <ul className="divide-y divide-border overflow-hidden rounded-[14px] border border-border bg-surface shadow-card">
          {recent.data?.map((c) => (
            <li key={c.id}>
              <Link
                to="/khach-hang/$customerId"
                params={{ customerId: c.id }}
                className="flex min-h-14 items-center justify-between gap-2 px-4 py-2 hover:bg-surface-2"
              >
                <span className="flex min-w-0 items-center gap-3">
                  {/* Given name's initial (Vietnamese names end with it), decorative. */}
                  <span
                    aria-hidden="true"
                    className="avatar"
                  >
                    {c.full_name.trim().split(/\s+/).at(-1)?.charAt(0).toUpperCase()}
                  </span>
                  <CustomerName name={c.full_name} gender={c.gender} className="font-semibold" />
                </span>
                <span className="shrink-0 text-sm whitespace-nowrap text-text-muted tabular">{c.phone}</span>
              </Link>
            </li>
          ))}
        </ul>
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
      <div className="grid content-start gap-4">
        <PageHeader breadcrumbs={[CUSTOMERS]} title={<span className="skeleton inline-block h-8 w-56 align-middle" />} />
        <div role="status" aria-label="Đang tải hồ sơ…" className="skeleton h-48" />
      </div>
    );
  if (customer.isError) {
    return (
      <div className="grid content-start gap-4">
        <PageHeader breadcrumbs={[CUSTOMERS]} title="Hồ sơ khách" />
        <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
          {messageFor(customer.error)}
        </p>
      </div>
    );
  }
  const c = customer.data;
  return (
    <div className="grid content-start gap-4">
      <PageHeader
        breadcrumbs={[CUSTOMERS]}
        title={<CustomerName name={c.full_name} gender={c.gender} showUnknown />}
        label={c.full_name}
        titleId="customer-name"
      />
      <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <CustomerProfile
          customer={customer.data}
          onChanged={(c) => {
            queryClient.setQueryData(customerQuery(customerId).queryKey, c);
          }}
        />
        <CustomerHistory customerId={customerId} me={me} />
      </div>
    </div>
  );
}
