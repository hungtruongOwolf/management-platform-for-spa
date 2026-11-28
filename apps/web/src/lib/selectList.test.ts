import { describe, expect, it } from "vitest";
import { isSelectListKey } from "./selectList";

describe("isSelectListKey", () => {
  const form = document.createElement("form");
  form.innerHTML = `
    <select><optgroup label="Nhóm"><option>Một</option></optgroup><option>Hai</option></select>
    <input>`;
  const select = form.querySelector("select");
  const [grouped, plain] = Array.from(form.querySelectorAll("option"));
  const input = form.querySelector("input");

  it("is true for keys from inside the open list (an option, grouped or not)", () => {
    expect(isSelectListKey({ target: grouped ?? null })).toBe(true);
    expect(isSelectListKey({ target: plain ?? null })).toBe(true);
  });

  it("is false for the closed select itself and for anything outside a select", () => {
    expect(isSelectListKey({ target: select })).toBe(false);
    expect(isSelectListKey({ target: input })).toBe(false);
    expect(isSelectListKey({ target: form })).toBe(false);
    expect(isSelectListKey({ target: null })).toBe(false);
  });
});
