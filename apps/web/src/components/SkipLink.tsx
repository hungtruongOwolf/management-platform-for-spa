/**
 * "Bỏ qua tới nội dung chính" (WCAG 2.4.1 Bypass Blocks, ADR-0019 D20): the first Tab stop, so a
 * keyboard user jumps past the sidebar and top bar. Focus moves itself instead of following
 * `#id`, which would put a hash on the router's URL. The target needs `tabIndex={-1}`.
 */
export function SkipLink({ target }: { target: string }) {
  return (
    <a
      href={`#${target}`}
      className="skip-link"
      onClick={(e) => {
        e.preventDefault();
        e.currentTarget.ownerDocument.getElementById(target)?.focus();
      }}
    >
      Bỏ qua tới nội dung chính
    </a>
  );
}
