import { useId, useRef, type KeyboardEvent } from "react";
import { useSearchState } from "../../lib/searchState";
import { ParticipantsPanel } from "./ParticipantsPanel";
import { PrizesPanel } from "./PrizesPanel";
import { SIDE_TABS, drawSearch, type SideTab } from "./search";
import type { LuckyDrawDetail, Prize } from "./types";
import { WinnersPanel } from "./WinnersPanel";

const TAB_LABEL: Record<SideTab, string> = { giai: "Giải thưởng", trung: "Trúng thưởng", nguoi: "Người tham gia" };

/**
 * The side column next to the wheel (EM 2026-11-28: "điều phối sắp xếp lại trang cho rõ ràng"):
 * one card of fixed size, three tabs (WAI-ARIA tabs, automatic activation, ← → Home End), the
 * chosen one in the URL (`tab`). Each tab keeps its actions on top and scrolls its list inside, so
 * no list, however long, grows the page.
 */
export function SidePanels({
  draw,
  current,
  locked,
  fresh,
  onPick,
  onForfeit,
  onSummary,
}: {
  draw: LuckyDrawDetail;
  current: Prize | null;
  locked: boolean;
  fresh: string | null;
  onPick: (prizeId: string) => void;
  onForfeit: (resultId: string) => void;
  onSummary: () => void;
}) {
  const [s, patch] = useSearchState(drawSearch);
  const id = useId();
  const tabs = useRef<Partial<Record<SideTab, HTMLButtonElement | null>>>({});
  const counts: Record<SideTab, number> = {
    giai: draw.prizes.length,
    trung: draw.results.length,
    nguoi: draw.participants.length,
  };
  const go = (tab: SideTab) => {
    patch({ tab });
    tabs.current[tab]?.focus();
  };
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const last = SIDE_TABS.length - 1;
    const to = { ArrowRight: i === last ? 0 : i + 1, ArrowLeft: i === 0 ? last : i - 1, Home: 0, End: last }[e.key];
    const tab = to === undefined ? undefined : SIDE_TABS[to];
    if (!tab) return;
    e.preventDefault();
    go(tab);
  };

  return (
    <section className="card ld-side" aria-label="Giải thưởng, người trúng và người tham gia">
      <div role="tablist" aria-label="Nội dung chương trình" className="tabs ld-tabs">
        {SIDE_TABS.map((tab, i) => (
          <button
            key={tab}
            ref={(el) => {
              tabs.current[tab] = el;
            }}
            type="button"
            role="tab"
            id={`${id}-${tab}`}
            aria-selected={s.tab === tab}
            aria-controls={`${id}-panel`}
            tabIndex={s.tab === tab ? 0 : -1}
            onClick={() => {
              patch({ tab });
            }}
            onKeyDown={(e) => {
              onKeyDown(e, i);
            }}
          >
            <span className="ld-tab-label">{TAB_LABEL[tab]}</span>{" "}
            <span className="ld-tab-count tabular">{counts[tab].toLocaleString("vi-VN")}</span>
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${s.tab}`} className="ld-tabpanel">
        {s.tab === "giai" ? (
          <PrizesPanel draw={draw} current={current} locked={locked} onPick={onPick} />
        ) : s.tab === "trung" ? (
          <WinnersPanel draw={draw} fresh={fresh} onForfeit={onForfeit} onSummary={onSummary} />
        ) : (
          <ParticipantsPanel draw={draw} />
        )}
      </div>
    </section>
  );
}
