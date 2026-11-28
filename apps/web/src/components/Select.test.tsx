import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Select, type SelectOption } from "./Select";

type Prize = "a" | "b" | "c" | "d";
const OPTIONS: SelectOption<Prize>[] = [
  { value: "a", label: "Chăm sóc cổ vai gáy" },
  { value: "b", label: "Hết rồi", disabled: true, reason: "Hết suất" },
  { value: "c", label: "Gội đầu dưỡng sinh" },
  { value: "d", label: "Xông hơi thảo dược" },
];

function Harness({ onChange }: { onChange: (v: Prize) => void }) {
  const [value, setValue] = useState<Prize>("a");
  return (
    <Select
      label="Giải đang quay"
      value={value}
      options={OPTIONS}
      onChange={(v) => {
        setValue(v);
        onChange(v);
      }}
    />
  );
}

const setup = () => {
  const onChange = vi.fn();
  const user = userEvent.setup();
  render(<Harness onChange={onChange} />);
  const box = screen.getByRole("combobox", { name: "Giải đang quay" });
  return { onChange, user, box };
};

describe("Select (APG select-only combobox)", () => {
  it("shows the value, opens on ↓ with the chosen option active, and keeps focus on the button", async () => {
    const { user, box } = setup();
    expect(box).toHaveTextContent("Chăm sóc cổ vai gáy");
    expect(box).toHaveAttribute("aria-expanded", "false");
    await user.tab();
    await user.keyboard("{ArrowDown}");
    expect(box).toHaveAttribute("aria-expanded", "true");
    const list = screen.getByRole("listbox", { name: "Giải đang quay" });
    const chosen = within(list).getByRole("option", { name: "Chăm sóc cổ vai gáy" });
    expect(chosen).toHaveAttribute("aria-selected", "true");
    expect(box).toHaveAttribute("aria-activedescendant", chosen.id);
    expect(box).toHaveFocus();
  });

  it("moves with the arrows, chooses with Enter, and lists a disabled option with its reason", async () => {
    const { onChange, user, box } = setup();
    await user.tab();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    const disabled = screen.getByRole("option", { name: /Hết rồi/ });
    expect(disabled).toHaveAttribute("aria-disabled", "true");
    expect(disabled).toHaveTextContent("Hết suất");
    expect(box).toHaveAttribute("aria-activedescendant", disabled.id);
    // A disabled option cannot be chosen: the list stays open.
    await user.keyboard("{Enter}");
    expect(onChange).not.toHaveBeenCalled();
    expect(box).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowDown}{Enter}");
    expect(onChange).toHaveBeenCalledWith("c");
    expect(box).toHaveAttribute("aria-expanded", "false");
    expect(box).toHaveTextContent("Gội đầu dưỡng sinh");
    expect(box).toHaveFocus();
  });

  it("jumps by typing, accents not needed; End / Home; Esc closes without a change", async () => {
    const { onChange, user, box } = setup();
    await user.tab();
    await user.keyboard("xo");
    expect(box).toHaveAttribute("aria-expanded", "true");
    expect(box).toHaveAttribute("aria-activedescendant", screen.getByRole("option", { name: "Xông hơi thảo dược" }).id);
    await user.keyboard("{Home}");
    expect(box).toHaveAttribute("aria-activedescendant", screen.getByRole("option", { name: "Chăm sóc cổ vai gáy" }).id);
    await user.keyboard("{End}{Escape}");
    expect(box).toHaveAttribute("aria-expanded", "false");
    expect(onChange).not.toHaveBeenCalled();
    expect(box).toHaveFocus();
  });

  it("Tab chooses the active option and moves on; a click chooses too", async () => {
    const { onChange, user, box } = setup();
    await user.tab();
    await user.keyboard("{End}{Tab}");
    expect(onChange).toHaveBeenLastCalledWith("d");
    expect(box).not.toHaveFocus();
    await user.click(box);
    await user.click(screen.getByRole("option", { name: "Gội đầu dưỡng sinh" }));
    expect(onChange).toHaveBeenLastCalledWith("c");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("closes on a press outside", async () => {
    const { user, box } = setup();
    await user.click(box);
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    await user.click(document.body);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });
});
