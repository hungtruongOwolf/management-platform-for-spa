import { useMemo, useState } from "react";
import { Icon } from "../../app/Icon";
import { RowMenu } from "../../components/RowMenu";
import { useSearchState } from "../../lib/searchState";
import { SearchField } from "../reports/filters";
import { deleteParticipant } from "./api";
import { GENDER_LABEL, filterCounts, filterPeople, personState, wins, type PeopleFilter, type Win } from "./draw";
import { PasteSheet } from "./PasteSheet";
import { PersonSheet } from "./PersonSheet";
import { PEOPLE_MAX } from "./roster";
import { PEOPLE_FILTERS, drawSearch } from "./search";
import { ConfirmSheet } from "./sheets";
import type { LuckyDrawDetail, Participant } from "./types";
import { useDrawWrite } from "./useDrawWrite";

const FILTER_LABEL: Record<PeopleFilter, string> = { all: "Tất cả", female: "Nữ", male: "Nam", won: "Đã trúng" };

type Open = { kind: "add" } | { kind: "paste" } | { kind: "edit"; person: Participant } | { kind: "delete"; person: Participant } | null;

function Status({ person, results }: { person: Participant; results: ReadonlyMap<string, Win> }) {
  const st = personState(person, results);
  if (st.kind === "won") return <span className="badge badge-ok">Đã trúng: {st.prize}</span>;
  if (st.kind === "absent") return <span className="badge badge-muted">Vắng mặt</span>;
  return <span className="badge badge-info">Chưa quay</span>;
}

/**
 * The "Người tham gia" tab (US2): the actions, filter chips with their counts and an accent-
 * insensitive search (both in the URL) stay on top; the people, in entry order, scroll inside a
 * fixed panel (EM 2026-11-28: "1 component kích thước cứng … scroll ở trong đó"), every one of
 * them, however many. Only someone without a result can be edited or deleted (AC2.4).
 */
export function ParticipantsPanel({ draw }: { draw: LuckyDrawDetail }) {
  const [s, patch] = useSearchState(drawSearch);
  const [open, setOpen] = useState<Open>(null);
  const close = () => {
    setOpen(null);
  };
  const results = useMemo(() => new Map(wins(draw).map((w) => [w.result.id, w])), [draw]);
  const counts = filterCounts(draw.participants, results);
  const rows = filterPeople(draw.participants, results, s.loc, s.q);
  const order = useMemo(() => new Map(draw.participants.map((p, i) => [p.id, i + 1])), [draw.participants]);
  const remove = useDrawWrite(draw.id, (personId: string) => deleteParticipant(draw.id, personId), close);
  const full = draw.participants.length >= PEOPLE_MAX;

  return (
    <>
      <div className="ld-panel-head">
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
        {draw.participants.length > 0 && (
          <>
            <div className="segmented ld-filter" role="group" aria-label="Lọc người tham gia">
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
          </>
        )}
      </div>

      <div className="ld-scroll" role="region" aria-label="Người tham gia" tabIndex={0}>
        {draw.participants.length === 0 ? (
          <p className="empty-note">
            Chưa có người tham gia. Bấm “Thêm người” để nhập từng người, hoặc “Dán danh sách” để dán hai cột Họ tên, Giới tính từ
            Excel.
          </p>
        ) : rows.length === 0 ? (
          <p className="empty-note">Không có ai khớp. Đổi bộ lọc hoặc xóa ô tìm (tìm không cần gõ dấu).</p>
        ) : (
          <ul className="ld-people" aria-label="Danh sách người tham gia">
            {rows.map((p) => (
              <li key={p.id} className="ld-person">
                <span className="ld-person-n tabular">{order.get(p.id)}</span>
                <span className="ld-person-main">
                  <span className="ld-person-name">{p.name}</span>
                  <span className="ld-person-meta">
                    <span className="gender-chip" data-gender={p.gender}>
                      {GENDER_LABEL[p.gender]}
                    </span>
                    <Status person={p} results={results} />
                  </span>
                </span>
                {p.result_id === null ? (
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
                ) : (
                  // Every row as tall as one with ⋯ (a list of mixed heights reads uneven).
                  <span aria-hidden="true" className="ld-menu-space" />
                )}
              </li>
            ))}
          </ul>
        )}
      </div>

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
    </>
  );
}
