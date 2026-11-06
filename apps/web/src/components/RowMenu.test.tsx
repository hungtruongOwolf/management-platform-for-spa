import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RowMenu } from "./RowMenu";

function setup() {
  const open = vi.fn();
  const cancel = vi.fn();
  render(
    <>
      <RowMenu
        label="Thao tác lượt 09:12 của Hà"
        items={[
          { label: "Mở hồ sơ khách", onSelect: open },
          { label: "Hủy lượt", onSelect: cancel, danger: true },
        ]}
      />
      <button type="button">Sau</button>
    </>,
  );
  const button = screen.getByRole("button", { name: "Thao tác lượt 09:12 của Hà" });
  return { open, cancel, button };
}

describe("RowMenu", () => {
  it("is a menu button: opens on the first item, arrows wrap, Esc returns to the button", async () => {
    const { button } = setup();
    const user = userEvent.setup();
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    const menu = screen.getByRole("menu", { name: "Thao tác lượt 09:12 của Hà" });
    const [first, second] = within(menu).getAllByRole("menuitem");
    expect(first).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(second).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(first).toHaveFocus();
    await user.keyboard("{End}");
    expect(second).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();
  });

  it("runs the chosen item and closes; a click outside closes without running anything", async () => {
    const { open, cancel, button } = setup();
    const user = userEvent.setup();
    await user.click(button);
    await user.click(screen.getByRole("menuitem", { name: "Hủy lượt" }));
    expect(cancel).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
    await user.click(button);
    await user.click(screen.getByRole("button", { name: "Sau" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(open).not.toHaveBeenCalled();
  });

  it("opens from the keyboard with ↓ and closes when Tab leaves", async () => {
    const { button } = setup();
    const user = userEvent.setup();
    button.focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Mở hồ sơ khách" })).toHaveFocus();
    await user.tab();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("stays open on a scroll that does not move its row, closes when the row moves", async () => {
    const { button } = setup();
    const user = userEvent.setup();
    let top = 500;
    vi.spyOn(button, "getBoundingClientRect").mockImplementation(() => new DOMRect(10, top, 44, 44));
    await user.click(button);
    window.dispatchEvent(new Event("scroll"));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    top = 380;
    window.dispatchEvent(new Event("scroll"));
    expect(await screen.findByRole("button", { name: "Thao tác lượt 09:12 của Hà" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
