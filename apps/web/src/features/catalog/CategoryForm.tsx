import type { ServiceCategory } from "@vpyd/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { fieldMessage, messageFor } from "../../lib/errors";
import { LazyRecordTrace } from "../record-history/lazy";
import { updateCategory } from "./api";
import { CATALOG_FIELDS } from "./messages";

/**
 * "Sửa nhóm" (owner, catalog.manage): renames one group of services, inline under its heading
 * like a service's edit form (a hairline above, no box). Only the name (spec record-history K48):
 * the group keeps its place, its `sort_order` sent back as read. Saving the name unchanged just
 * closes (a save would add a "sửa nhóm dịch vụ" line that changed nothing). The server
 * re-validates (`name` taken, empty, too long).
 */
export function CategoryForm({
  category,
  onDone,
  traced = false,
}: {
  category: ServiceCategory;
  onDone: () => void;
  /** "Ai ghi, ai sửa" as the form's last element above its buttons (ADR-0022, K39; `canTrace`, UX only). */
  traced?: boolean;
}) {
  const queryClient = useQueryClient();
  const id = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(category.name);
  const [touched, setTouched] = useState(false);
  const mutation = useMutation({
    mutationFn: () => updateCategory(category.id, { name: name.trim(), sort_order: category.sort_order }),
    onSuccess: async () => {
      // The group's name shows on the price list, in every service picker and on the plans.
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
      onDone();
    },
  });
  useEffect(() => {
    input.current?.focus();
  }, []);
  const title = `Sửa nhóm: ${category.name}`;
  const nameError = fieldMessage(mutation.error, "name", CATALOG_FIELDS);
  const problem = (touched && name.trim() === "" ? "Nhập tên nhóm." : undefined) ?? nameError;
  return (
    <form
      aria-label={title}
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        setTouched(true);
        if (name.trim() === "" || mutation.isPending) return;
        if (name.trim() === category.name) onDone();
        else mutation.mutate();
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onDone();
      }}
      className="row-form"
    >
      <h3 ref={heading} className="panel-title" tabIndex={traced ? -1 : undefined}>
        {title}
      </h3>
      <div className="form-grid">
        <div className="field span-2">
          <label htmlFor={`${id}-name`}>Tên nhóm dịch vụ</label>
          <input
            id={`${id}-name`}
            ref={input}
            value={name}
            autoComplete="off"
            aria-invalid={problem ? true : undefined}
            aria-describedby={problem ? `${id}-error` : `${id}-hint`}
            onChange={(e) => {
              setName(e.target.value);
            }}
            className="input max-w-md"
          />
          {problem ? (
            <span id={`${id}-error`} className="field-error">
              {problem}
            </span>
          ) : (
            <span id={`${id}-hint`} className="field-hint">
              Chỉ đổi tên; nhóm giữ nguyên vị trí trong bảng giá.
            </span>
          )}
        </div>
        {traced && (
          // The edit form's last element, above its buttons (design record-history §2.10); on 403
          // focus goes to the form's title, which never depends on the right (K45).
          <div className="span-2">
            <LazyRecordTrace
              record={{ type: "service_category", id: category.id }}
              noun={`nhóm dịch vụ ${category.name}`}
              focusOnHide={() => heading.current}
            />
          </div>
        )}
        {mutation.isError && !nameError && (
          <p role="alert" className="alert span-2">
            {messageFor(mutation.error)}
          </p>
        )}
        <div className="button-row span-2">
          <button type="submit" aria-busy={mutation.isPending || undefined} className="btn btn-primary">
            Lưu nhóm dịch vụ
          </button>
          <button type="button" onClick={onDone} className="btn btn-secondary">
            Hủy
          </button>
        </div>
      </div>
    </form>
  );
}
