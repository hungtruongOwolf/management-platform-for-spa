/**
 * True for a key pressed inside the open list of a select. With the customizable select (styles.css
 * opts every select into `appearance: base-select`, components.css "Select list") the open list is
 * part of the page: its keys (↑/↓ move, Enter picks, Esc closes the list) reach the page from the
 * focused <option>, where the browser's native list kept them to itself. A form's or a page's
 * shortcut (Esc closes the form, Esc drops the customer) leaves those keys to the list.
 */
export function isSelectListKey(e: { target: EventTarget | null }): boolean {
  const { target } = e;
  return (
    target instanceof Element &&
    !(target instanceof HTMLSelectElement) &&
    target.closest("select") !== null
  );
}
