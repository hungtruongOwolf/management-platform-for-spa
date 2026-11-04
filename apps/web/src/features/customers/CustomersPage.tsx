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
    <section aria-labelledby="customers-title" className="space-y-4">
      <h1 id="customers-title" className="font-brand text-2xl text-primary">
        Khách hàng
      </h1>
      <div className="rounded-[14px] border border-border bg-surface p-4">
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
        {recent.data?.length === 0 && (
          <p className="text-text-muted">Chưa có khách nào.</p>
        )}
        <ul className="divide-y divide-border rounded-[10px] border border-border bg-surface">
          {recent.data?.map((c) => (
            <li key={c.id}>
              <Link
                to="/khach-hang/$customerId"
                params={{ customerId: c.id }}
                className="flex min-h-11 justify-between gap-2 px-3 py-2 hover:bg-surface-2"
              >
                <span className="font-semibold">{c.full_name}</span>
                <span className="text-sm text-text-muted">
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

/** /khach-hang/:customerId — profile with edit. */
export function CustomerPage({ customerId }: { customerId: string }) {
  const queryClient = useQueryClient();
  const customer = useQuery(customerQuery(customerId));
  if (customer.isPending)
    return (
      <p role="status" className="text-text-muted">
        Đang tải hồ sơ…
      </p>
    );
  if (customer.isError) {
    return (
      <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-danger">
        {messageFor(customer.error)}
      </p>
    );
  }
  return (
    <div className="space-y-3">
      <Link to="/khach-hang" className="text-sm text-text-muted underline">
        ← Danh sách khách
      </Link>
      <CustomerProfile
        customer={customer.data}
        onChanged={(c) => {
          queryClient.setQueryData(customerQuery(customerId).queryKey, c);
        }}
      />
    </div>
  );
}
