import type { ReactNode } from "react";
import { spaTime } from "../../lib/format";
import { EligibleChip } from "./Stage";
import { GENDER_LABEL, wins, type Win } from "./draw";
import type { LuckyDrawDetail, Prize } from "./types";

export interface PrizeWins {
  prize: Prize;
  wins: Win[];
}

/** Results grouped by prize, in prize order; within a prize, in spin order. Prizes without a result are left out. */
export function winsByPrize(draw: LuckyDrawDetail): PrizeWins[] {
  const all = wins(draw);
  return [...draw.prizes]
    .sort((a, b) => a.position - b.position)
    .map((prize) => ({ prize, wins: all.filter((w) => w.result.prize_id === prize.id) }))
    .filter((g) => g.wins.length > 0);
}

/**
 * One winner as a card row (EM: "làm đẹp hơn … đừng làm kiểu cái danh sách"): the spin number in a
 * gold ring, the name large, the gender chip, the time; a forfeited result is dimmed with "Vắng mặt".
 * Shared by the winners card, the summary sheet and the presentation, so all three read the same.
 */
export function WinRow({ win, fresh = false, action }: { win: Win; fresh?: boolean; action?: ReactNode }) {
  const absent = win.result.forfeited_at !== null;
  return (
    <li className={`ld-win${absent ? " is-absent" : ""}${fresh ? " is-fresh" : ""}`}>
      <span className="ld-seq tabular">
        <span className="sr-only">Lượt </span>
        {win.result.seq}
      </span>
      <div className="ld-win-main">
        <p className="ld-win-name">
          <span className="ld-win-who">{win.person?.name ?? ""}</span>
          {win.person && (
            <span className="gender-chip" data-gender={win.person.gender}>
              {GENDER_LABEL[win.person.gender]}
            </span>
          )}
          {absent && <span className="badge badge-muted">Vắng mặt</span>}
        </p>
        <p className="ld-win-meta">
          lúc <time dateTime={win.result.drawn_at}>{spaTime(win.result.drawn_at)}</time>
        </p>
      </div>
      {action}
    </li>
  );
}

/** A prize's heading over its winners: name, audience chip, "Đã trao a/b". */
export function PrizeHeading({ prize, level = 3, id }: { prize: Prize; level?: 3 | 4; id?: string }) {
  const Heading = level === 4 ? "h4" : "h3";
  return (
    <div className="ld-group-head">
      <Heading id={id} className="ld-group-name">
        {prize.name}
      </Heading>
      <EligibleChip prize={prize} />
      <span className="ld-group-count tabular">
        Đã trao {prize.awarded}/{prize.quantity}
      </span>
    </div>
  );
}
