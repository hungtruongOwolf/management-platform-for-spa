import { useMemo, useState } from "react";
import { Icon } from "../../app/Icon";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { RowMenu } from "../../components/RowMenu";
import { useSearchState } from "../../lib/searchState";
import { SearchField } from "../reports/filters";
import { deleteParticipant } from "./api";
import { GENDER_LABEL, filterCounts, filterPeople, personState, wins, type PeopleFilter } from "./draw";
import { PasteSheet } from "./PasteSheet";
import { PersonSheet } from "./PersonSheet";
import { PEOPLE_MAX } from "./roster";
import { PEOPLE_FILTERS, drawSearch } from "./search";
import { ConfirmSheet } from "./sheets";
import type { LuckyDrawDetail, Participant } from "./types";
import { useDrawWrite } from "./useDrawWrite";
import { useShowMore } from "./useShowMore";

const FILTER_LABEL: Record<PeopleFilter, string> = { all: "Tất cả", female: "Nữ", male: "Nam", won: "Đã trúng" };

type Open = { kind: "add" } | { kind: "paste" } | { kind: "edit"; person: Participant } | { kind: "delete"; person: Participant } | null;

/**
 * "Người tham gia" (US2): filter chips with their counts, an accent-insensitive search (both in the
 * URL), the list in entry order, 100 rows at a time. Only someone without a result can be edited
 * or deleted (AC2.4): the others have no ⋯ at all.
 */
export function ParticipantsCard({ draw }: { draw: LuckyDrawDetail }) {
  const [s, patch] = useSearchState(drawSearch);
  const [open, setOpen] = useState<Open>(null);
  const close = () => {
    setOpen(null);
  };
  const results = useMemo(() => new Map(wins(draw).map((w) => [w.result.id, w])), [draw]);
  const counts = filterCounts(draw.participants, results);
  const rows = filterPeople(draw.participants, results, s.loc, s.q);
  const { shown, more } = useShowMore(rows, `${draw.id}|${s.loc}|${s.q}`);
  const order = useMemo(() => new Map(draw.participants.map((p, i) => [p.id, i + 1])), [draw.participants]);
  const remove = useDrawWrite(draw.id, (personId: string) => deleteParticipant(draw.id, personId), close);
  const full = draw.participants.length >= PEOPLE_MAX;

  return (
    <section className="card panel" aria-labelledby="ld-people-title">
      <div className="panel-head">
        <h2 id="ld-people-title" className="panel-title">
          Người tham gia <span className="ld-count tabular">{counts.all.toLocaleString("vi-VN")}</span>
        </h2>
        {!full && (
          <div className="button-row">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                setOpen({ kind: "add" });
              }}
            >
              <Icon name="plus" className="size-4" />
              Thêm người
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                setOpen({ kind: "paste" });
              }}
            >
              Dán danh sách
            </button>
          </div>
        )}
      </div>

      {draw.participants.length === 0 ? (
        <p className="empty-note">
          Chưa có người tham gia. Bấm “Thêm người” để nhập từng người, hoặc “Dán danh sách” để dán hai cột Họ tên, Giới tính từ
          Excel.
        </p>
      ) : (
        <>
          <div className="ld-people-tools">
            <div className="segmented chip-row" role="group" aria-label="Lọc người tham gia">
              {PEOPLE_FILTERS.map((f) => (
                <button
                  key={f}
                  type="button"
                  aria-pressed={s.loc === f}
                  onClick={() => {
                    patch({ loc: f });
                  }}
                >
                  {FILTER_LABEL[f]}{" "}
                  <span className="ld-chip-count tabular">{counts[f].toLocaleString("vi-VN")}</span>
                </button>
              ))}
            </div>
            <SearchField
              label="Tìm theo họ tên"
              value={s.q}
              minLength={1}
              onCommit={(q) => {
                patch({ q });
              }}
            />
          </div>
          {rows.length === 0 ? (
            <p className="empty-note">Không có ai khớp. Đổi bộ lọc hoặc xóa ô tìm (tìm không cần gõ dấu).</p>
          ) : (
            <ResponsiveTable<Participant>
              caption="Danh sách người tham gia"
              rows={shown}
              rowKey={(p) => p.id}
              columns={[
                { id: "n", header: "#", numeric: true, card: "hidden", width: "4rem", cell: (p) => order.get(p.id) },
                { id: "name", header: "Họ tên", card: "title", cell: (p) => <span className="font-semibold">{p.name}</span> },
                {
                  id: "gender",
                  header: "Giới tính",
                  card: "subtitle",
                  width: "7rem",
                  cell: (p) => (
                    <span className="gender-chip" data-gender={p.gender}>
                      {GENDER_LABEL[p.gender]}
                    </span>
                  ),
                },
                {
                  id: "state",
                  header: "Trạng thái",
                  // Phone card: name and ⋯ on the first line, gender and status under it.
                  card: "subtitle",
                  cell: (p) => {
                    const st = personState(p, results);
                    if (st.kind === "won") return <span className="badge badge-ok">Đã trúng: {st.prize}</span>;
                    if (st.kind === "absent") return <span className="badge badge-muted">Vắng mặt</span>;
                    return <span className="badge badge-info">Chưa quay</span>;
                  },
                },
                {
                  id: "actions",
                  header: <span className="sr-only">Thao tác</span>,
                  card: "aside",
                  width: "3.5rem",
                  cell: (p) =>
                    p.result_id === null && (
                      <RowMenu
                        label={`Thao tác với ${p.name}`}
                        items={[
                          {
                            label: "Sửa",
                            onSelect: () => {
                              setOpen({ kind: "edit", person: p });
                            },
                          },
                          {
                            label: "Xóa",
                            danger: true,
                            onSelect: () => {
                              remove.reset();
                              setOpen({ kind: "delete", person: p });
                            },
                          },
                        ]}
                      />
                    ),
                },
              ]}
            />
          )}
          {more}
        </>
      )}

      {open?.kind === "add" && <PersonSheet drawId={draw.id} onClose={close} />}
      {open?.kind === "edit" && <PersonSheet drawId={draw.id} person={open.person} onClose={close} />}
      {open?.kind === "paste" && <PasteSheet draw={draw} onClose={close} />}
      {open?.kind === "delete" && (
        <ConfirmSheet
          title={`Xóa ${open.person.name}?`}
          confirm="Xóa người tham gia"
          danger
          mutation={remove}
          onConfirm={() => {
            remove.mutate(open.person.id);
          }}
          onClose={close}
        >
          {open.person.name} chưa được quay. Xóa xong, người này không còn trong danh sách quay.
        </ConfirmSheet>
      )}
    </section>
  );
}
