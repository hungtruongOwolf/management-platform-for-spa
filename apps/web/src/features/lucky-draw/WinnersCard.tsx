import { RowMenu } from "../../components/RowMenu";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { spaTime } from "../../lib/format";
import { downloadCsv } from "./csv";
import { GENDER_LABEL, wins, type Win } from "./draw";
import { useShowMore } from "./useShowMore";
import type { LuckyDrawDetail } from "./types";

/**
 * "Danh sách trúng thưởng" (spec §5): newest spin first, so the winner just drawn is on top. A
 * forfeited result stays, muted, with "Vắng mặt" (AC5.1); a present one offers "Vắng mặt, trả giải".
 */
export function WinnersCard({
  draw,
  onForfeit,
  onSummary,
}: {
  draw: LuckyDrawDetail;
  onForfeit: (resultId: string) => void;
  onSummary: () => void;
}) {
  const all = wins(draw).reverse();
  const { shown, more } = useShowMore(all, draw.id);
  return (
    <section className="card panel" aria-labelledby="ld-winners-title">
      <div className="panel-head">
        <h2 id="ld-winners-title" className="panel-title">
          Danh sách trúng thưởng
        </h2>
        {all.length > 0 && (
          <div className="button-row">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => {
                downloadCsv(draw);
              }}
            >
              Tải danh sách (CSV)
            </button>
            <button type="button" className="btn btn-secondary" onClick={onSummary}>
              Xem tổng kết
            </button>
          </div>
        )}
      </div>
      {all.length === 0 ? (
        <p className="empty-note">Chưa có ai trúng. Chọn giải rồi bấm “Quay”: người trúng hiện ở đây.</p>
      ) : (
        <>
          <ResponsiveTable<Win>
            caption="Danh sách trúng thưởng"
            rows={shown}
            rowKey={(w) => w.result.id}
            rowClassName={(w) => (w.result.forfeited_at === null ? undefined : "ld-row-absent")}
            columns={[
              { id: "seq", header: "Lượt", numeric: true, cell: (w) => w.result.seq },
              {
                id: "who",
                header: "Người trúng",
                card: "title",
                cell: (w) => (
                  <span className="ld-who">
                    <span className="font-semibold">{w.person?.name ?? ""}</span>
                    {w.person && <span className="sr-only">, {GENDER_LABEL[w.person.gender]}</span>}
                    {w.result.forfeited_at !== null && <span className="badge badge-muted">Vắng mặt</span>}
                  </span>
                ),
              },
              { id: "prize", header: "Giải", card: "subtitle", cell: (w) => w.prize?.name ?? "" },
              { id: "at", header: "Lúc", numeric: true, cell: (w) => spaTime(w.result.drawn_at) },
              {
                id: "actions",
                header: <span className="sr-only">Thao tác</span>,
                // Top right of a phone card, not a row of its own.
                card: "aside",
                cell: (w) =>
                  w.result.forfeited_at === null ? (
                    <RowMenu
                      label={`Thao tác với lượt ${String(w.result.seq)}, ${w.person?.name ?? ""}`}
                      items={[
                        {
                          label: "Vắng mặt, trả giải",
                          onSelect: () => {
                            onForfeit(w.result.id);
                          },
                        },
                      ]}
                    />
                  ) : (
                    <span aria-hidden="true" className="ld-menu-space" />
                  ),
              },
            ]}
          />
          {more}
        </>
      )}
    </section>
  );
}
