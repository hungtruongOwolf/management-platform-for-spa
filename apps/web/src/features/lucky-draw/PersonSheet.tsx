import { useId, useRef, useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { fieldMessage } from "../../lib/errors";
import { addParticipants, updateParticipant } from "./api";
import { GENDER_LABEL } from "./draw";
import { DRAW_FIELDS } from "./messages";
import { NAME_MAX, chars, cleanName } from "./roster";
import { FormError } from "./sheets";
import type { Gender, Participant, ParticipantWrite } from "./types";
import { useDrawWrite } from "./useDrawWrite";

const GENDERS: readonly Gender[] = ["female", "male"];

/**
 * "Thêm người" / "Sửa người tham gia" (AC2.1, AC2.4): name and Nữ / Nam, no gender chosen up front.
 * Adding keeps the sheet open for the next name (a list is typed one after another); "Xong" closes.
 */
export function PersonSheet({
  drawId,
  person,
  onClose,
}: {
  drawId: string;
  person?: Participant;
  onClose: () => void;
}) {
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(person?.name ?? "");
  const [gender, setGender] = useState<Gender | null>(person?.gender ?? null);
  const [touched, setTouched] = useState(false);
  const [added, setAdded] = useState<string | null>(null);
  const save = useDrawWrite(
    drawId,
    (body: ParticipantWrite) => (person ? updateParticipant(drawId, person, body) : addParticipants(drawId, [body])),
    (_, body) => {
      if (person) {
        onClose();
        return;
      }
      setAdded(body.name);
      setName("");
      setGender(null);
      setTouched(false);
      input.current?.focus();
    },
  );
  const clean = cleanName(name);
  const nameProblem =
    (touched && clean === "" ? "Nhập họ tên." : undefined) ??
    (chars(clean) > NAME_MAX ? `Họ tên dài quá ${String(NAME_MAX)} ký tự.` : undefined) ??
    fieldMessage(save.error, person ? "name" : "participants[0].name", DRAW_FIELDS);
  const genderProblem =
    (touched && gender === null ? "Chọn Nữ hoặc Nam." : undefined) ??
    fieldMessage(save.error, person ? "gender" : "participants[0].gender", { ...DRAW_FIELDS, invalid: "Chọn Nữ hoặc Nam." });

  return (
    <BottomSheet
      open
      onClose={onClose}
      title={person ? `Sửa người tham gia ${person.name}` : "Thêm người tham gia"}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            {person ? "Hủy" : "Xong"}
          </button>
          <button type="submit" form={id} className="btn btn-primary" aria-busy={save.isPending || undefined}>
            {person ? "Lưu người tham gia" : "Thêm người"}
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
          if (clean === "" || chars(clean) > NAME_MAX || gender === null || save.isPending) return;
          save.mutate({ name: clean, gender });
        }}
      >
        <div className="field">
          <label htmlFor={`${id}-name`}>Họ tên</label>
          <input
            ref={input}
            id={`${id}-name`}
            className="input"
            value={name}
            autoComplete="off"
            autoFocus
            aria-invalid={nameProblem ? true : undefined}
            aria-describedby={nameProblem ? `${id}-name-error` : undefined}
            onChange={(e) => {
              setName(e.target.value);
              setAdded(null);
            }}
          />
          {nameProblem && (
            <span id={`${id}-name-error`} className="field-error">
              {nameProblem}
            </span>
          )}
        </div>
        <div className="field">
          <span id={`${id}-gender`}>Giới tính</span>
          <div
            role="group"
            aria-labelledby={`${id}-gender`}
            aria-describedby={genderProblem ? `${id}-gender-error` : undefined}
            className="segmented justify-self-start"
          >
            {GENDERS.map((g) => (
              <button
                key={g}
                type="button"
                aria-pressed={gender === g}
                onClick={() => {
                  setGender(g);
                }}
              >
                {GENDER_LABEL[g]}
              </button>
            ))}
          </div>
          {genderProblem && (
            <span id={`${id}-gender-error`} className="field-error">
              {genderProblem}
            </span>
          )}
        </div>
        <FormError
          error={save.error}
          fields={person ? ["name", "gender"] : ["participants[0].name", "participants[0].gender"]}
        />
        {added && (
          <p role="status" className="note">
            Đã thêm {added}. Nhập người tiếp theo, hoặc bấm “Xong”.
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
