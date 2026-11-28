import { useState } from "react";
import { Icon } from "../../app/Icon";
import { RowMenu } from "../../components/RowMenu";
import { deletePrize } from "./api";
import { prizeBlock, prizeDrawn } from "./draw";
import { PrizeSheet } from "./PrizeSheet";
import { ConfirmSheet } from "./sheets";
import { EligibleChip } from "./Stage";
import type { LuckyDrawDetail, Prize } from "./types";
import { useDrawWrite } from "./useDrawWrite";

const PRIZES_MAX = 50;

type Open = { kind: "add" } | { kind: "edit"; prize: Prize } | { kind: "delete"; prize: Prize } | null;

/**
 * The "Giải thưởng" tab (US3, AC4.1): each prize is a choice for the stage (`aria-pressed`), with its
 * audience, the slots left, and why it cannot be drawn when it cannot. Edit / delete sit behind ⋯;
 * a prize with a result cannot be deleted, so its menu has no "Xóa".
 */
export function PrizesPanel({
  draw,
  current,
  locked,
  onPick,
}: {
  draw: LuckyDrawDetail;
  current: Prize | null;
  /** A spin is running: the prize stays as it is. */
  locked: boolean;
  onPick: (prizeId: string) => void;
}) {
  const [open, setOpen] = useState<Open>(null);
  const close = () => {
    setOpen(null);
  };
  const remove = useDrawWrite(draw.id, (prizeId: string) => deletePrize(draw.id, prizeId), close);

  return (
    <>
      <div className="ld-panel-head">
        {draw.prizes.length < PRIZES_MAX && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setOpen({ kind: "add" });
            }}
          >
            <Icon name="plus" className="size-4" />
            Thêm giải thưởng
          </button>
        )}
      </div>
      <div className="ld-scroll" role="region" aria-label="Các giải thưởng" tabIndex={0}>
      {draw.prizes.length === 0 ? (
        <p className="empty-note">Chưa có giải thưởng. Bấm “Thêm giải thưởng”: tên giải, dành cho ai, số suất.</p>
      ) : (
        <ul className="ld-prizes" aria-label="Chọn giải để quay">
          {draw.prizes.map((prize) => {
            const block = prizeBlock(prize);
            const drawn = prizeDrawn(draw, prize.id);
            return (
              <li key={prize.id} className="ld-prize">
                <button
                  type="button"
                  className="ld-prize-pick"
                  aria-pressed={current?.id === prize.id}
                  aria-disabled={block !== null || locked || undefined}
                  data-blocked={block !== null || undefined}
                  onClick={() => {
                    if (block === null && !locked) onPick(prize.id);
                  }}
                >
                  <span className="ld-prize-head">
                    <span className="ld-prize-name">{prize.name}</span>
                    {current?.id === prize.id && <Icon name="check" className="ld-prize-check size-4" />}
                  </span>
                  {prize.description && <span className="ld-prize-desc">{prize.description}</span>}
                  <span className="ld-prize-meta">
                    <EligibleChip prize={prize} />
                    <span className="tabular">
                      Còn {prize.remaining}/{prize.quantity} suất
                    </span>
                    {block && <span className="badge badge-muted">{block}</span>}
                  </span>
                </button>
                <RowMenu
                  label={`Thao tác với giải ${prize.name}`}
                  items={[
                    {
                      label: "Sửa giải",
                      onSelect: () => {
                        setOpen({ kind: "edit", prize });
                      },
                    },
                    ...(drawn
                      ? []
                      : [
                          {
                            label: "Xóa giải",
                            danger: true,
                            onSelect: () => {
                              remove.reset();
                              setOpen({ kind: "delete", prize });
                            },
                          },
                        ]),
                  ]}
                />
              </li>
            );
          })}
        </ul>
      )}
      </div>
      {open?.kind === "add" && <PrizeSheet drawId={draw.id} drawn={false} onClose={close} />}
      {open?.kind === "edit" && (
        <PrizeSheet drawId={draw.id} prize={open.prize} drawn={prizeDrawn(draw, open.prize.id)} onClose={close} />
      )}
      {open?.kind === "delete" && (
        <ConfirmSheet
          title={`Xóa giải ${open.prize.name}?`}
          confirm="Xóa giải"
          danger
          mutation={remove}
          onConfirm={() => {
            remove.mutate(open.prize.id);
          }}
          onClose={close}
        >
          Giải này chưa có ai trúng. Xóa xong, giải không còn trong chương trình.
        </ConfirmSheet>
      )}
    </>
  );
}
