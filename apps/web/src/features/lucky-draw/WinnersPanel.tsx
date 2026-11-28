import { useId } from "react";
import { RowMenu } from "../../components/RowMenu";
import { downloadCsv } from "./csv";
import type { LuckyDrawDetail } from "./types";
import { PrizeHeading, WinRow, winsByPrize } from "./WinRow";

/**
 * The "Trúng thưởng" tab (spec §5; EM: "làm đẹp hơn … đừng làm kiểu cái danh sách"): winners grouped
 * by prize in prize order, each a card row, in a fixed panel that scrolls inside (EM 2026-11-28). A
 * forfeited result stays, dimmed, with "Vắng mặt" (AC5.1); a present one offers "Vắng mặt, trả giải"
 * behind ⋯. The win just revealed (`fresh`) flashes once.
 */
export function WinnersPanel({
  draw,
  fresh,
  onForfeit,
  onSummary,
}: {
  draw: LuckyDrawDetail;
  fresh: string | null;
  onForfeit: (resultId: string) => void;
  onSummary: () => void;
}) {
  const id = useId();
  const groups = winsByPrize(draw);
  return (
    <>
      {groups.length > 0 && (
        <div className="ld-panel-head">
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
        </div>
      )}
      <div className="ld-scroll" role="region" aria-label="Danh sách trúng thưởng" tabIndex={0}>
        {groups.length === 0 ? (
          <p className="empty-note">Chưa có ai trúng. Chọn giải rồi bấm “Quay”: người trúng hiện ở đây.</p>
        ) : (
          <div className="ld-groups">
            {groups.map(({ prize, wins }) => (
              <section key={prize.id} className="ld-group" aria-labelledby={`${id}-${prize.id}`}>
                <PrizeHeading prize={prize} id={`${id}-${prize.id}`} />
                <ul className="ld-wins" aria-label={`Người trúng giải ${prize.name}`}>
                  {wins.map((w) => (
                    <WinRow
                      key={w.result.id}
                      win={w}
                      fresh={w.result.id === fresh}
                      action={
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
                          // Every row as tall as one with ⋯.
                          <span aria-hidden="true" className="ld-menu-space" />
                        )
                      }
                    />
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
