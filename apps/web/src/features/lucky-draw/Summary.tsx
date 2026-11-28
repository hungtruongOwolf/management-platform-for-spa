import { BottomSheet } from "../../components/BottomSheet";
import { downloadCsv } from "./csv";
import { GENDER_LABEL, summaryByPrize, type Win } from "./draw";
import type { LuckyDrawDetail } from "./types";

const who = (w: Win) => (w.person ? `${w.person.name} (${GENDER_LABEL[w.person.gender]})` : "");

/**
 * "Tổng kết chương trình" (AC6.1): how many slots went out, the winners prize by prize (prize order,
 * then spin order), what is left of a prize and why, and the absent winners on a line of their own.
 */
export function SummaryBody({ draw, headingLevel = 3 }: { draw: LuckyDrawDetail; headingLevel?: 3 | 4 }) {
  const summary = summaryByPrize(draw);
  const Heading = headingLevel === 4 ? "h4" : "h3";
  return (
    <div className="ld-summary">
      <p>
        <strong className="tabular">{summary.awarded}</strong> suất đã trao
        {draw.finished ? ": chương trình đã xong." : "."}
      </p>
      {summary.prizes.map(({ prize, winners }) => (
        <section key={prize.id} className="ld-summary-prize" aria-label={prize.name}>
          <Heading className="ld-summary-title">
            {prize.name}{" "}
            <span className="ld-summary-count tabular">
              {prize.awarded}/{prize.quantity} suất
            </span>
          </Heading>
          {winners.length > 0 ? (
            <ol className="ld-summary-list">
              {winners.map((w) => (
                <li key={w.result.id}>
                  <span className="ld-summary-seq tabular">Lượt {w.result.seq}</span> {who(w)}
                </li>
              ))}
            </ol>
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
