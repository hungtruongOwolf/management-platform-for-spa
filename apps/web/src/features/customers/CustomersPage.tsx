import {
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { GENDER_LABEL, customerQuery, recentQuery } from "./api";
import { CustomerForm } from "./CustomerForm";
import { CustomerProfile } from "./CustomerProfile";
import { CustomerSearch } from "./CustomerSearch";
import { Icon } from "../../app/Icon";
import { CustomerHistory } from "../reports/CustomerHistory";

/** /khach-hang — find or register a customer. */
export function CustomersPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const navigate = useNavigate();
  const [creating, setCreating] = useState<string | null>(null);
  const recent = useQuery(recentQuery);
  const open = (id: string) =>
    void navigate({
      to: "/khach-hang/$customerId",
      params: { customerId: id },
    });

  return (
    <section aria-labelledby="customers-title" className="grid content-start gap-4">
      <div>
        <p className="eyebrow">Hồ sơ khách</p>
        <h1 id="customers-title" className="page-title">
          Khách hàng
        </h1>
      </div>
      <div className="card p-4">
        <CustomerSearch
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
                  <span className="font-semibold">{c.full_name}</span>
                </span>
                <span className="text-sm text-text-muted tabular">
                  {c.phone}
                  {c.gender ? ` · ${GENDER_LABEL[c.gender] ?? ""}` : ""}
                </span>
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
  if (customer.isPending)
    return (
      <div role="status" aria-label="Đang tải hồ sơ…" className="grid content-start gap-3">
        <div className="skeleton h-5 w-36" />
        <div className="skeleton h-48" />
      </div>
    );
  if (customer.isError) {
    return (
      <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
        {messageFor(customer.error)}
      </p>
    );
  }
  return (
    <div className="grid content-start gap-3">
      <Link to="/khach-hang" className="inline-flex min-h-11 items-center gap-1 text-sm text-text-muted hover:text-primary">
        <Icon name="chevron-left" className="size-4" /> Danh sách khách
      </Link>
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
