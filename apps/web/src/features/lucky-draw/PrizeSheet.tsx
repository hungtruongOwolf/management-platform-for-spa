import { useId, useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { createPrize, updatePrize } from "./api";
import { ELIGIBLE_CHOICE } from "./draw";
import { DRAW_FIELDS } from "./messages";
import { chars } from "./roster";
import { FormError } from "./sheets";
import type { Eligible, Prize, PrizeWrite } from "./types";
import { useDrawWrite } from "./useDrawWrite";

const NAME_MAX = 140;
const NOTE_MAX = 250;
const QUANTITY_MAX = 1000;
const AUDIENCES: readonly Eligible[] = ["all", "female", "male"];

/**
 * "Thêm giải thưởng" / "Sửa giải" (US3): name, "Dành cho", slots, an optional description. A prize
 * already drawn keeps its audience and never goes below the slots already given (AC3.2); the
 * server re-checks both.
 */
export function PrizeSheet({
  drawId,
  prize,
  drawn,
  onClose,
}: {
  drawId: string;
  /** The prize to edit; none = a new one. */
  prize?: Prize;
  /** The prize has a result (forfeited included): its audience is fixed. */
  drawn: boolean;
  onClose: () => void;
}) {
  const id = useId();
  const [name, setName] = useState(prize?.name ?? "");
  const [eligible, setEligible] = useState<Eligible>(prize?.eligible ?? "all");
  const [quantity, setQuantity] = useState(prize ? String(prize.quantity) : "1");
  const [note, setNote] = useState(prize?.description ?? "");
  const [touched, setTouched] = useState(false);
  const save = useDrawWrite(drawId, (body: PrizeWrite) => (prize ? updatePrize(drawId, prize, body) : createPrize(drawId, body)), onClose);

  const awarded = prize?.awarded ?? 0;
  const count = Number(quantity);
  const nameProblem =
    (touched && name.trim() === "" ? "Nhập tên giải." : undefined) ??
    (chars(name.trim()) > NAME_MAX ? `Tên dài quá ${String(NAME_MAX)} ký tự.` : undefined) ??
    fieldMessage(save.error, "name", DRAW_FIELDS);
  const quantityProblem =
    (touched && !(Number.isInteger(count) && count >= 1 && count <= QUANTITY_MAX)
      ? `Nhập số suất từ 1 đến ${QUANTITY_MAX.toLocaleString("vi-VN")}.`
      : undefined) ??
    (touched && count < awarded ? `Đã trao ${String(awarded)} suất: không nhập ít hơn ${String(awarded)}.` : undefined) ??
    (save.error instanceof ApiError && save.error.code === "lucky_draw.quantity_below_awarded"
      ? messageFor(save.error)
      : fieldMessage(save.error, "quantity", DRAW_FIELDS));
  const noteProblem =
    (chars(note.trim()) > NOTE_MAX ? `Mô tả dài quá ${String(NOTE_MAX)} ký tự.` : undefined) ??
    fieldMessage(save.error, "description", DRAW_FIELDS);
  const audienceProblem = fieldMessage(save.error, "eligible", { ...DRAW_FIELDS, invalid: "Chọn một đối tượng: cả nam và nữ, chỉ nữ hoặc chỉ nam." });
  const quantityRefused = save.error instanceof ApiError && save.error.code === "lucky_draw.quantity_below_awarded";

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={prize ? `Sửa giải ${prize.name}` : "Thêm giải thưởng"}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Hủy
          </button>
          <button type="submit" form={id} className="btn btn-primary" aria-busy={save.isPending || undefined}>
            {prize ? "Lưu giải thưởng" : "Thêm giải thưởng"}
          </button>
        </>
      }
    >
      <form
        id={id}
        noValidate
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          const ok =
            name.trim() !== "" &&
            chars(name.trim()) <= NAME_MAX &&
            Number.isInteger(count) &&
            count >= Math.max(1, awarded) &&
            count <= QUANTITY_MAX &&
            chars(note.trim()) <= NOTE_MAX;
          if (!ok || save.isPending) return;
          save.mutate({ name: name.trim(), description: note.trim() === "" ? null : note.trim(), eligible, quantity: count });
        }}
      >
        <div className="field">
          <label htmlFor={`${id}-name`}>Tên giải</label>
          <input
            id={`${id}-name`}
            className="input"
            value={name}
            autoComplete="off"
            autoFocus={!prize}
            aria-invalid={nameProblem ? true : undefined}
            aria-describedby={`${id}-name-help`}
            onChange={(e) => {
              setName(e.target.value);
            }}
          />
          <span id={`${id}-name-help`} className={nameProblem ? "field-error" : "field-hint"}>
            {nameProblem ?? "Ví dụ: Chăm sóc cổ vai gáy."}
          </span>
        </div>
        <div className="field">
          <span id={`${id}-audience`}>Dành cho</span>
          <div
            role="group"
            aria-labelledby={`${id}-audience`}
            aria-describedby={drawn || audienceProblem ? `${id}-audience-help` : undefined}
            className="segmented justify-self-start"
          >
            {AUDIENCES.map((a) => (
              <button
                key={a}
                type="button"
                aria-pressed={eligible === a}
                disabled={drawn && eligible !== a}
                onClick={() => {
                  setEligible(a);
                }}
              >
                {ELIGIBLE_CHOICE[a]}
              </button>
            ))}
          </div>
          {(drawn || audienceProblem) && (
            <span id={`${id}-audience-help`} className={audienceProblem ? "field-error" : "field-hint"}>
              {audienceProblem ?? "Giải đã có kết quả quay nên không đổi được “Dành cho”."}
            </span>
          )}
        </div>
        <div className="field">
          <label htmlFor={`${id}-qty`}>Số suất</label>
          <input
            id={`${id}-qty`}
            className="input ld-qty"
            type="number"
            inputMode="numeric"
            min={Math.max(1, awarded)}
            max={QUANTITY_MAX}
            step={1}
            value={quantity}
            aria-invalid={quantityProblem || quantityRefused ? true : undefined}
            aria-describedby={`${id}-qty-help`}
            onChange={(e) => {
              setQuantity(e.target.value);
            }}
          />
          <span id={`${id}-qty-help`} className={quantityProblem ? "field-error" : "field-hint"}>
            {quantityProblem ?? (awarded > 0 ? `Đã trao ${String(awarded)} suất.` : "Mỗi lượt quay trao một suất.")}
          </span>
        </div>
        <div className="field">
          <label htmlFor={`${id}-note`}>Mô tả (không bắt buộc)</label>
          <textarea
            id={`${id}-note`}
            value={note}
            rows={2}
            aria-invalid={noteProblem ? true : undefined}
            aria-describedby={noteProblem ? `${id}-note-error` : undefined}
            onChange={(e) => {
              setNote(e.target.value);
            }}
          />
          {noteProblem && (
            <span id={`${id}-note-error`} className="field-error">
              {noteProblem}
            </span>
          )}
        </div>
        <FormError error={quantityRefused ? null : save.error} fields={["name", "quantity", "description", "eligible"]} />
      </form>
    </BottomSheet>
  );
}
