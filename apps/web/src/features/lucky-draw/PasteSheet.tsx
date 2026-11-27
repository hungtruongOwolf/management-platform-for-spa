import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useDeferredValue, useId, useMemo, useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { LIST_KEY, addParticipants, drawKey } from "./api";
import { BATCH_MAX, PEOPLE_MAX, batches, parseRoster } from "./roster";
import { FormError } from "./sheets";
import type { LuckyDrawDetail } from "./types";

const count = (n: number) => n.toLocaleString("vi-VN");
/** How many problem lines the preview lists before "và n dòng khác". */
const SHOWN = 20;

/**
 * "Dán danh sách" (AC2.2): one person per line, pasted from Excel or typed. The preview below the
 * box counts who will be added and names every bad line by its number; nothing is sent while a
 * line is wrong. A long list goes in batches of 500, one after another; when one fails, the lines
 * already saved leave the box, so trying again never adds anyone twice.
 */
export function PasteSheet({ draw, onClose }: { draw: LuckyDrawDetail; onClose: () => void }) {
  const id = useId();
  const queryClient = useQueryClient();
  const [text, setText] = useState("");
  const [sent, setSent] = useState(0);
  const typed = useDeferredValue(text);
  const existing = useMemo(() => draw.participants.map((p) => p.name), [draw.participants]);
  const roster = useMemo(() => parseRoster(typed, existing), [typed, existing]);
  const room = PEOPLE_MAX - draw.participants.length;
  const over = roster.people.length > room;
  const women = roster.people.filter((p) => p.gender === "female").length;

  const save = useMutation({
    mutationFn: async () => {
      const lines = text.split(/\r\n|\n|\r/);
      const parsed = parseRoster(text, existing);
      let done = 0;
      try {
        for (const batch of batches(parsed.people)) {
          const next = await addParticipants(draw.id, batch);
          queryClient.setQueryData(drawKey(draw.id), next);
          done += batch.length;
          setSent(done);
        }
      } catch (error) {
        // Keep only what was not saved: from the first unsent person's line on.
        const from = parsed.lines[done];
        if (done > 0 && from !== undefined) setText(lines.slice(from - 1).join("\n"));
        throw error;
      } finally {
        void queryClient.invalidateQueries({ queryKey: LIST_KEY, exact: true });
      }
    },
    onSuccess: onClose,
  });

  const blocked = roster.errors.length > 0 || roster.people.length === 0 || over;
  const total = roster.people.length;

  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title="Dán danh sách người tham gia"
      description="Mỗi người một dòng: họ tên, rồi Nữ hoặc Nam."
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Hủy
          </button>
          <button
            type="submit"
            form={id}
            className="btn btn-primary"
            aria-busy={save.isPending || undefined}
            aria-disabled={blocked || undefined}
          >
            {roster.people.length > 0 ? `Thêm ${count(roster.people.length)} người` : "Thêm người"}
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
          if (blocked || save.isPending || typed !== text) return;
          setSent(0);
          save.mutate();
        }}
      >
        <div className="field">
          <label htmlFor={`${id}-list`}>Danh sách</label>
          <textarea
            id={`${id}-list`}
            className="ld-paste"
            value={text}
            rows={8}
            spellCheck={false}
            autoFocus
            aria-describedby={`${id}-help ${id}-preview`}
            onChange={(e) => {
              setText(e.target.value);
            }}
          />
          <span id={`${id}-help`} className="field-hint">
            Ví dụ: “Nguyễn Thị Lan, Nữ” hoặc “Trần Văn Minh; Nam”. Dán hai cột Họ tên và Giới tính từ Excel cũng được.
          </span>
        </div>

        <div id={`${id}-preview`} className="grid gap-3" aria-live="polite">
          {roster.people.length > 0 && (
            <p className="text-sm">
              Sẽ thêm <strong className="tabular">{count(roster.people.length)}</strong> người ({count(women)} nữ,{" "}
              {count(roster.people.length - women)} nam)
              {roster.people.length > BATCH_MAX && `, gửi thành ${String(batches(roster.people).length)} lần`}.
            </p>
          )}
          {over && (
            <p className="alert">
              Chương trình nhận tối đa {count(PEOPLE_MAX)} người: còn chỗ cho {count(room)} người.
            </p>
          )}
          {roster.errors.length > 0 && <Notes tone="alert" title="Sửa các dòng sau rồi bấm thêm lại:" notes={roster.errors} />}
          {roster.warnings.length > 0 && (
            <Notes tone="callout" title="Trùng tên (vẫn được thêm, kiểm tra lại nếu nhập nhầm):" notes={roster.warnings} />
          )}
        </div>

        {save.isPending && total > BATCH_MAX && (
          <p role="status" className="note tabular">
            Đã thêm {count(sent)}/{count(total)} người…
          </p>
        )}
        {save.isError && sent > 0 && (
          <p className="note">Đã thêm {count(sent)} người. Các dòng còn lại vẫn ở trong ô: bấm thêm lại khi sẵn sàng.</p>
        )}
        <FormError error={save.error} />
      </form>
    </BottomSheet>
  );
}

function Notes({ tone, title, notes }: { tone: "alert" | "callout"; title: string; notes: readonly { line: number; text: string }[] }) {
  return (
    <div className={`${tone} ld-note-box`}>
      <p className="font-semibold">{title}</p>
      <ul className="ld-notes">
        {notes.slice(0, SHOWN).map((n) => (
          <li key={n.line}>{n.text}</li>
        ))}
        {notes.length > SHOWN && <li>… và {count(notes.length - SHOWN)} dòng khác.</li>}
      </ul>
    </div>
  );
}
