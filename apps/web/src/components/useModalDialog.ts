import { useEffect, useRef } from "react";

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
