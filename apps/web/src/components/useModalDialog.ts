import { useEffect, useRef, type MouseEvent, type PointerEvent } from "react";

/**
 * Opens/closes a `<dialog>` with `showModal()` (focus trap, Esc, backdrop from the browser). Own
 * module: the shell's drawer uses it on the startup path, the sheet component stays lazy.
 */
export function useModalDialog(open: boolean) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
    } else if (!open && d.open) {
      if (typeof d.close === "function") d.close();
      else d.removeAttribute("open");
    }
  }, [open]);
  return ref;
}

/**
 * Close a modal `<dialog>` on a backdrop click (it lands on the dialog element itself), but only
 * when the press started there too: a text selection dragged out of a field and released over the
 * backdrop also fires `click` on the dialog, and closing then would throw away the form.
 */
export function useBackdropClose(onClose: () => void) {
  const pressedOnBackdrop = useRef(false);
  return {
    onPointerDown: (e: PointerEvent<HTMLDialogElement>) => {
      pressedOnBackdrop.current = e.target === e.currentTarget;
    },
    onClick: (e: MouseEvent<HTMLDialogElement>) => {
      if (pressedOnBackdrop.current && e.target === e.currentTarget) onClose();
      pressedOnBackdrop.current = false;
    },
  };
}
