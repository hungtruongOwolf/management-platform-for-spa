import { BottomSheet } from "../../components/BottomSheet";
import { downloadCsv } from "./csv";
import { GENDER_LABEL, summaryByPrize, type Win } from "./draw";
import type { LuckyDrawDetail } from "./types";
import { PrizeHeading, WinRow } from "./WinRow";

const who = (w: Win) => (w.person ? `${w.person.name} (${GENDER_LABEL[w.person.gender]})` : "");

/**
 * "Tổng kết chương trình" (AC6.1): how many slots went out, the winners prize by prize (prize order,
 * then spin order) as the same card rows as the winners list, what is left of a prize and why, and
 * the absent winners on a line of their own.
 */
export function SummaryBody({ draw, headingLevel = 3 }: { draw: LuckyDrawDetail; headingLevel?: 3 | 4 }) {
  const summary = summaryByPrize(draw);
  return (
    <div className="ld-summary">
      <p>
        <strong className="tabular">{summary.awarded}</strong> suất đã trao
        {draw.finished ? ": chương trình đã xong." : "."}
      </p>
      {summary.prizes.map(({ prize, winners }) => (
        <section key={prize.id} className="ld-group" aria-label={prize.name}>
          <PrizeHeading prize={prize} level={headingLevel} />
          {winners.length > 0 ? (
            <ul className="ld-wins">
              {winners.map((w) => (
                <WinRow key={w.result.id} win={w} />
              ))}
            </ul>
          ) : (
            <p className="text-sm text-text-muted">Chưa ai trúng giải này.</p>
          )}
          {prize.remaining > 0 && (
            <p className="text-sm text-text-muted">
              {prize.eligible_count === 0
                ? `Còn ${String(prize.remaining)} suất “${prize.name}” — không còn người phù hợp.`
                : `Còn ${String(prize.remaining)} suất chưa quay.`}
            </p>
          )}
        </section>
      ))}
      {summary.absent.length > 0 && (
        <p className="text-sm">
          <strong>Vắng mặt, đã trả giải:</strong> {summary.absent.map((w) => `${who(w)} · ${w.prize?.name ?? ""}`).join("; ")}.
        </p>
      )}
    </div>
  );
}

export function SummarySheet({ draw, onClose }: { draw: LuckyDrawDetail; onClose: () => void }) {
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title="Tổng kết chương trình"
      description={draw.name}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Đóng
          </button>
          {draw.results.length > 0 && (
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                downloadCsv(draw);
              }}
            >
              Tải danh sách (CSV)
            </button>
          )}
        </>
      }
    >
      <SummaryBody draw={draw} />
    </BottomSheet>
  );
}
