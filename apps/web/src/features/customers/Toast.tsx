import { useCallback, useEffect, useState } from "react";

const SHOW_MS = 4000;

/**
 * Confirmation toast of the customer screens (design-system.md "Quy tắc" 10): top centre, so it
 * never covers sticky actions. The live region stays mounted (announcements stay reliable) and
 * has no box while idle.
 */
export function useToast(): [(text: string) => void, React.ReactNode] {
  const [text, setText] = useState("");
  useEffect(() => {
    if (!text) return;
    const t = setTimeout(() => {
      setText("");
    }, SHOW_MS);
    return () => {
      clearTimeout(t);
    };
  }, [text]);
  const show = useCallback((next: string) => {
    setText(next);
  }, []);
  return [
    show,
    <p key="toast" role="status" className={`khach-toast ${text ? "is-shown" : ""}`}>
      {text}
    </p>,
  ];
}
