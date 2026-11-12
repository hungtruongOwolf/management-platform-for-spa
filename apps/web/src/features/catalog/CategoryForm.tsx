import type { ServiceCategory } from "@vpyd/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { BottomSheet } from "../../components/BottomSheet";
import { ApiError, api } from "../../lib/api";
import { addMessages, fieldMessage, messageFor } from "../../lib/errors";
import { LazyRecordTrace } from "../record-history/lazy";
import { updateCategory } from "./api";
import { CATALOG_FIELDS } from "./messages";

// Only this form meets it; kept out of the startup chunk (frontend.md "Perf budget").
addMessages({
  "catalog.category_in_use":
    "Không xóa được: nhóm này đang có dịch vụ (có thể vừa được thêm ở máy khác). Chỉ xóa được nhóm chưa có dịch vụ nào.",
});

/**
 * Deletes a group with no service in it (`catalog.manage`, spec record-history K68). Here and not in
 * `api.ts`, which the front desk loads at startup: only the price list's chunk carries it.
 */
async function deleteCategory(id: string): Promise<void> {
  await api(`/service-categories/${encodeURIComponent(id)}`, { method: "DELETE" });
}

/**
 * "Sửa nhóm" (owner, catalog.manage): renames one group of services, inline under its heading
 * like a service's edit form (a hairline above, no box). Only the name (spec record-history K48):
 * the group keeps its place, its `sort_order` sent back as read. Saving the name unchanged just
 * closes (a save would add a "sửa nhóm dịch vụ" line that changed nothing). The server
 * re-validates (`name` taken, empty, too long).
 *
 * "Xóa nhóm" (K68) shows only while the group has no service, on sale or not (`deletable`, from
 * the list the price list already has; UX only: the server refuses a group in use with `409`).
 */
export function CategoryForm({
  category,
  onDone,
  traced = false,
  deletable = false,
  onDeleted,
}: {
  category: ServiceCategory;
  onDone: () => void;
  /** "Ai ghi, ai sửa" as the form's last element above its buttons (ADR-0022, K39; `canTrace`, UX only). */
  traced?: boolean;
  /** The group has no service: "Xóa nhóm" is offered. */
  deletable?: boolean;
  /** The group is gone (the catalogue already refetched): the page closes the form and moves focus. */
  onDeleted?: () => void;
}) {
  const queryClient = useQueryClient();
  const id = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const removeButton = useRef<HTMLButtonElement>(null);
  const [name, setName] = useState(category.name);
  const [touched, setTouched] = useState(false);
  const [confirming, setConfirming] = useState(false);
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
    <>
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
            {deletable && (
              <button
                ref={removeButton}
                type="button"
                aria-label={`Xóa nhóm ${category.name}`}
                onClick={() => {
                  setConfirming(true);
                }}
                className="btn btn-ghost text-danger"
              >
                Xóa nhóm
              </button>
            )}
          </div>
        </div>
      </form>
      {/* Outside the form: Esc in the sheet closes the sheet, not the form behind it. */}
      {confirming && (
        <DeleteCategory
          category={category}
          onClose={() => {
            // The sheet leaves the page open: back on the button that opened it (gone after a
            // refused delete that showed the group's new service: then the form's title).
            flushSync(() => {
              setConfirming(false);
            });
            (removeButton.current ?? heading.current)?.focus();
          }}
          onDeleted={onDeleted ?? onDone}
        />
      )}
    </>
  );
}

/**
 * Confirms the delete (the app's confirm sheet, as "Xóa khoản này"). A group someone just filled
 * from another device is refused (`409`) and the price list refetched, so that service shows and
 * the button goes; a group already gone (`404`) counts as deleted.
 */
function DeleteCategory({
  category,
  onClose,
  onDeleted,
}: {
  category: ServiceCategory;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const queryClient = useQueryClient();
  const remove = useMutation({
    mutationFn: async () => {
      try {
        await deleteCategory(category.id);
      } catch (error) {
        if (!(error instanceof ApiError && error.status === 404)) throw error;
      }
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
      onDeleted();
    },
    onError: async () => {
      await queryClient.invalidateQueries({ queryKey: ["catalog"] });
    },
  });
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Xóa nhóm ${category.name}?`}
      footer={
        <>
          <button type="button" className="btn btn-secondary" onClick={onClose}>
            Thôi
          </button>
          <button
            type="button"
            className="btn btn-danger"
            aria-busy={remove.isPending || undefined}
            onClick={() => {
              if (!remove.isPending) remove.mutate();
            }}
          >
            Xóa nhóm
          </button>
        </>
      }
    >
      <p className="text-sm">Nhóm này chưa có dịch vụ nào. Xóa xong, nhóm không còn trên bảng giá.</p>
      {remove.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(remove.error)}
        </p>
      )}
    </BottomSheet>
  );
}
