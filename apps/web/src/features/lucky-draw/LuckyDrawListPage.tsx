import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { spaToday } from "../../lib/format";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { createDraw, drawsQuery, putDraw } from "./api";
import { STATUS_BADGE, STATUS_LABEL } from "./draw";
import "./messages";
import { NameSheet } from "./sheets";
import type { LuckyDrawSummary } from "./types";
import "./lucky-draw.css";

/** "2026-11-27T…Z" → "27/11/2026", the day on the spa's calendar. */
export function spaDay(iso: string): string {
  const [y, m, d] = spaToday(new Date(iso)).split("-");
  return `${d ?? ""}/${m ?? ""}/${y ?? ""}`;
}

const count = (n: number) => n.toLocaleString("vi-VN");

/**
 * /vong-quay (US1): every prize draw of the spa, newest first, and "Tạo chương trình". Creating one
 * opens it straight away. The owner's screen: `lucky_draw.manage` opens it, the API refuses others.
 */
export function LuckyDrawListPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const list = useQuery(drawsQuery);
  const [creating, setCreating] = useState(false);
  const create = useMutation({
    mutationFn: createDraw,
    onSuccess: (draw) => {
      putDraw(queryClient, draw);
      void navigate({ to: "/vong-quay/$drawId", params: { drawId: draw.id } });
    },
  });
  const open = () => {
    create.reset();
    setCreating(true);
  };

  return (
    <div className="grid content-start gap-5">
      <PageHeader title="Vòng quay may mắn">
        <button type="button" className="btn btn-primary" onClick={open}>
          <Icon name="plus" className="size-4" />
          Tạo chương trình
        </button>
      </PageHeader>
      <div className="card rt-host overflow-hidden">
        {list.isPending ? (
          <RowsSkeleton rows={4} />
        ) : list.isError ? (
          <div className="p-4">
            <ErrorState error={list.error} onRetry={() => void list.refetch()} />
          </div>
        ) : list.data.length === 0 ? (
          <EmptyState title="Chưa có chương trình nào">
            Bấm “Tạo chương trình”, đặt tên như “Tri ân 20/11”, rồi thêm người tham gia và giải thưởng.
          </EmptyState>
        ) : (
          <ResponsiveTable<LuckyDrawSummary>
            caption="Chương trình vòng quay"
            rows={list.data}
            rowKey={(d) => d.id}
            stretchLink
            columns={[
              {
                id: "name",
                header: "Tên chương trình",
                card: "title",
                cell: (d) => (
                  <Link to="/vong-quay/$drawId" params={{ drawId: d.id }} className="name-link">
                    {d.name}
                  </Link>
                ),
              },
              { id: "created", header: "Ngày tạo", card: "subtitle", cell: (d) => <span className="tabular">{spaDay(d.created_at)}</span> },
              { id: "people", header: "Người tham gia", numeric: true, cell: (d) => count(d.participant_count) },
              {
                id: "awarded",
                header: "Đã trao / tổng suất",
                numeric: true,
                cell: (d) => `${count(d.awarded)}/${count(d.prize_slots)}`,
              },
              {
                id: "status",
                header: "Trạng thái",
                card: "aside",
                cell: (d) => <span className={`badge ${STATUS_BADGE[d.status]}`}>{STATUS_LABEL[d.status]}</span>,
              },
            ]}
          />
        )}
      </div>
      {creating && (
        <NameSheet
          title="Tạo chương trình"
          submit="Tạo chương trình"
          mutation={create}
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
    </div>
  );
}
