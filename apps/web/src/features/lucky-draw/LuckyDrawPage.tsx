import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { useCallback, useRef, useState } from "react";
import { Icon } from "../../app/Icon";
import { PageHeader } from "../../app/PageHeader";
import { RowMenu } from "../../components/RowMenu";
import { ApiError } from "../../lib/api";
import { ErrorState } from "../reports/ui";
import { LIST_KEY, deleteDraw, drawKey, drawQuery, forfeitResult, renameDraw } from "./api";
import { currentPrize, wins } from "./draw";
import "./messages";
import { ParticipantsCard } from "./ParticipantsCard";
import { Presentation } from "./Presentation";
import { PrizesCard } from "./PrizesCard";
import { ConfirmSheet, NameSheet } from "./sheets";
import { Stage } from "./Stage";
import { SummaryBody, SummarySheet } from "./Summary";
import type { LuckyDrawDetail, SpinOutcome } from "./types";
import { useDrawWrite } from "./useDrawWrite";
import { useSpin } from "./useSpin";
import { WinnerDialog } from "./WinnerDialog";
import { WinnersCard } from "./WinnersCard";
import "./lucky-draw.css";

// The list ships in this chunk too: the router imports one module for both screens.
export { LuckyDrawListPage } from "./LuckyDrawListPage";

const CRUMBS = [{ label: "Vòng quay may mắn", to: "/vong-quay" }];
/** Program ids are UUIDs; anything else in the URL (a hand-edited link) is not asked for. */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** /vong-quay/$drawId — one program (US2–US7). */
export function LuckyDrawPage() {
  const { drawId } = useParams({ from: "/authed/vong-quay/$drawId" });
  const valid = UUID.test(drawId);
  const draw = useQuery({ ...drawQuery(drawId), enabled: valid });
  if (!valid) return <Missing />;
  if (draw.isPending) return <ProgramSkeleton />;
  if (draw.isError) {
    // 404: deleted, or another spa's.
    if (draw.error instanceof ApiError && draw.error.status === 404) return <Missing />;
    return (
      <div className="grid content-start gap-5">
        <PageHeader title="Chương trình" breadcrumbs={CRUMBS} />
        <ErrorState error={draw.error} onRetry={() => void draw.refetch()} />
      </div>
    );
  }
  return <Program draw={draw.data} />;
}

function Missing() {
  return (
    <div className="grid content-start gap-5">
      <PageHeader title="Chương trình" breadcrumbs={CRUMBS} />
      <p role="alert" className="note">
        Không tìm thấy chương trình này (có thể đã bị xóa).{" "}
        <Link to="/vong-quay" className="link underline">
          Về danh sách chương trình
        </Link>
      </p>
    </div>
  );
}

type Open = "rename" | "delete" | "summary" | { forfeit: string } | null;

function Program({ draw }: { draw: LuckyDrawDetail }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [open, setOpen] = useState<Open>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [presenting, setPresenting] = useState(false);
  const presentButton = useRef<HTMLButtonElement>(null);
  const close = () => {
    setOpen(null);
  };
  /** The result whose popup is open: each spin that lands announces its winner (EM). */
  const [winner, setWinner] = useState<string | null>(null);
  const spinButton = useRef<HTMLButtonElement>(null);
  const onRevealed = useCallback((outcome: SpinOutcome) => {
    setWinner(outcome.result.id);
  }, []);
  const spin = useSpin(draw.id, onRevealed);
  const prize = currentPrize(draw.prizes, picked);
  const backToSpin = () => {
    requestAnimationFrame(() => spinButton.current?.focus());
  };
  const closeWinner = () => {
    setWinner(null);
    backToSpin();
  };

  const rename = useDrawWrite(draw.id, (name: string) => renameDraw(draw, name), close);
  const forfeit = useDrawWrite(draw.id, (resultId: string) => forfeitResult(draw.id, resultId), close);
  const remove = useMutation({
    mutationFn: async () => {
      try {
        await deleteDraw(draw.id);
      } catch (error) {
        // Already gone (another tab): the same end.
        if (!(error instanceof ApiError && error.status === 404)) throw error;
      }
    },
    onSuccess: async () => {
      // Leave first: dropping the program while this screen still watches it would refetch a 404.
      await navigate({ to: "/vong-quay" });
      queryClient.removeQueries({ queryKey: drawKey(draw.id), exact: true });
      void queryClient.invalidateQueries({ queryKey: LIST_KEY, exact: true });
    },
  });
  const askForfeit = (resultId: string) => {
    forfeit.reset();
    setOpen({ forfeit: resultId });
  };
  const forfeiting = open !== null && typeof open === "object" ? wins(draw).find((w) => w.result.id === open.forfeit) : undefined;
  const stopPresenting = () => {
    setPresenting(false);
    requestAnimationFrame(() => presentButton.current?.focus());
  };

  return (
    <div className="ld-page">
      <PageHeader title={draw.name} breadcrumbs={CRUMBS}>
        <button
          ref={presentButton}
          type="button"
          className="btn btn-secondary"
          aria-disabled={spin.busy || undefined}
          onClick={() => {
            if (!spin.busy) setPresenting(true);
          }}
        >
          <Icon name="monitor" className="size-4" />
          Trình chiếu
        </button>
        <RowMenu
          label="Thao tác với chương trình"
          items={[
            {
              label: "Đổi tên",
              onSelect: () => {
                rename.reset();
                setOpen("rename");
              },
            },
            {
              label: "Xóa chương trình",
              danger: true,
              onSelect: () => {
                remove.reset();
                setOpen("delete");
              },
            },
          ]}
        />
      </PageHeader>

      {draw.finished && (
        <section className="card panel" aria-labelledby="ld-summary-title">
          <h2 id="ld-summary-title" className="panel-title">
            Tổng kết chương trình
          </h2>
          <SummaryBody draw={draw} />
        </section>
      )}

      <div className="ld-grid">
        {presenting ? (
          <div className="ld-stage chrome" aria-hidden="true" />
        ) : (
          <Stage draw={draw} prize={prize} spin={spin} buttonRef={spinButton} />
        )}
        <div className="ld-side">
          <PrizesCard draw={draw} current={prize} locked={spin.busy} onPick={setPicked} />
          <WinnersCard
            draw={draw}
            fresh={spin.shownId}
            onForfeit={askForfeit}
            onSummary={() => {
              setOpen("summary");
            }}
          />
        </div>
      </div>
      <ParticipantsCard draw={draw} />

      {presenting && (
        <Presentation draw={draw} prize={prize} spin={spin} buttonRef={spinButton} onPick={setPicked} onClose={stopPresenting} />
      )}
      {winner !== null && (
        // After the presentation in the tree: shown later, it sits above it in the top layer.
        <WinnerDialog
          draw={draw}
          resultId={winner}
          next={prize}
          big={presenting}
          onNext={() => {
            setWinner(null);
            backToSpin();
            if (prize) spin.start(prize.id);
          }}
          onSummary={() => {
            setWinner(null);
            setOpen("summary");
          }}
          onForfeit={(resultId) => {
            setWinner(null);
            askForfeit(resultId);
          }}
          onClose={closeWinner}
        />
      )}
      {open === "summary" && <SummarySheet draw={draw} onClose={close} />}
      {open === "rename" && (
        <NameSheet title="Đổi tên chương trình" submit="Lưu tên" initial={draw.name} mutation={rename} onClose={close} />
      )}
      {open === "delete" && (
        <ConfirmSheet
          title={`Xóa chương trình “${draw.name}”?`}
          confirm="Xóa chương trình"
          danger
          mutation={remove}
          onConfirm={() => {
            remove.mutate();
          }}
          onClose={close}
        >
          Danh sách người tham gia, giải và kết quả sẽ bị xóa, không khôi phục được.
        </ConfirmSheet>
      )}
      {forfeiting && (
        <ConfirmSheet
          title={`Trả giải “${forfeiting.prize?.name ?? ""}” của ${forfeiting.person?.name ?? ""}?`}
          confirm="Vắng mặt, trả giải"
          danger
          mutation={forfeit}
          onConfirm={() => {
            forfeit.mutate(forfeiting.result.id);
          }}
          onClose={close}
        >
          Suất này quay lại cho lượt sau; {forfeiting.person?.name ?? ""} không được quay nữa.
        </ConfirmSheet>
      )}
    </div>
  );
}

/** The program's shape while it loads (rulebook §9): header, stage, two cards, the list. */
function ProgramSkeleton() {
  return (
    <div className="ld-page" role="status" aria-label="Đang tải chương trình">
      <div className="grid gap-2">
        <div className="skeleton h-4 w-40" />
        <div className="skeleton h-8 w-64" />
      </div>
      <div className="ld-grid">
        <div className="skeleton ld-stage-skeleton" />
        <div className="ld-side">
          <div className="skeleton h-56" />
          <div className="skeleton h-40" />
        </div>
      </div>
      <div className="skeleton h-64" />
    </div>
  );
}
