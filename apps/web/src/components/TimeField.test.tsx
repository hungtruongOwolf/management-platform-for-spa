import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { TimeField, parseHm } from "./TimeField";

describe("parseHm", () => {
  it("reads 24-hour times in the ways the desk types them", () => {
    expect(parseHm("15:00")).toBe("15:00");
    expect(parseHm("1500")).toBe("15:00");
    expect(parseHm("9:30")).toBe("09:30");
    expect(parseHm("930")).toBe("09:30");
    expect(parseHm("9")).toBe("09:00");
    expect(parseHm("15h30")).toBe("15:30");
    expect(parseHm("24:00")).toBeNull();
    expect(parseHm("12:60")).toBeNull();
    expect(parseHm("3 PM")).toBeNull();
  });
});

function Controlled({ onChange }: { onChange: (hm: string) => void }) {
  const [value, setValue] = useState("");
  return (
    <>
      <label>
        Giờ
        <TimeField
          value={value}
          onChange={(hm) => {
            setValue(hm);
            onChange(hm);
          }}
        />
      </label>
      <button
        type="button"
        onClick={() => {
          setValue("");
        }}
      >
        Dùng giờ hiện tại
      </button>
    </>
  );
}

describe("TimeField", () => {
  it("typing 15:00 gives exactly 15:00, never an AM/PM time (EM live test: 15:58)", async () => {
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const box = screen.getByRole("textbox", { name: "Giờ" });
    expect(box).not.toHaveAttribute("type", "time");
    await userEvent.type(box, "15:00");
    expect(box).toHaveValue("15:00");
    expect(onChange).toHaveBeenLastCalledWith("15:00");
  });

  it("inserts the colon for bare digits and pads on blur", async () => {
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const box = screen.getByRole("textbox", { name: "Giờ" });
    await userEvent.type(box, "1430");
    expect(box).toHaveValue("14:30");
    expect(onChange).toHaveBeenLastCalledWith("14:30");
    await userEvent.clear(box);
    await userEvent.type(box, "9");
    await userEvent.tab();
    expect(box).toHaveValue("09:00");
    expect(onChange).toHaveBeenLastCalledWith("09:00");
  });

  it("flags an impossible time and follows a reset from the parent", async () => {
    const onChange = vi.fn();
    render(<Controlled onChange={onChange} />);
    const box = screen.getByRole("textbox", { name: "Giờ" });
    await userEvent.type(box, "25:00");
    await userEvent.tab();
    expect(box).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("24 giờ");
    expect(onChange).not.toHaveBeenCalled();
    await userEvent.clear(box);
    await userEvent.type(box, "08:15");
    await userEvent.click(screen.getByRole("button", { name: "Dùng giờ hiện tại" }));
    expect(box).toHaveValue("");
  });
});
