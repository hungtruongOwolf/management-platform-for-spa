import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { WRITE } from "../support/fixtures";
import { sidewaysOverflow } from "../support/responsive";
import { expectAccessible } from "../support/run";

/**
 * "Xóa nhóm" on the price list (spec record-history K68): the owner deletes a group of services
 * added by mistake from its "Sửa nhóm" form, after a confirm sheet; focus lands on the page title
 * (the group was the last one). A group with services offers no delete and the API refuses one.
 * Each width deletes the marked group it added, so a run leaves nothing behind.
 */

test.use({ storageState: ownerState });

const stamp = String(Date.now()).slice(-6);

test("'Xóa nhóm': an empty group goes after a confirm, at 1280 and 360 px", async ({ page }) => {
  for (const width of [1280, 360]) {
    await page.setViewportSize({ width, height: 900 });
    const name = `Nhóm xóa thử ${stamp} ${String(width)}`;
    const added = await page.request.post("/api/v1/service-categories", { headers: WRITE, data: { name } });
    expect(added.status(), await added.text()).toBe(201);
    await page.goto("/bang-gia");
    await page.getByRole("button", { name: `Sửa nhóm ${name}` }).click();
    const form = page.getByRole("form", { name: `Sửa nhóm: ${name}` });
    const remove = form.getByRole("button", { name: `Xóa nhóm ${name}` });
    await expect(remove).toHaveText("Xóa nhóm");
    await expectAccessible(page, `price-list-group-form-${String(width)}`);

    await remove.click();
    const sheet = page.getByRole("dialog", { name: `Xóa nhóm ${name}?` });
    await expect(sheet).toContainText("Nhóm này chưa có dịch vụ nào.");
    if (width === 360) expect(await sidewaysOverflow(page)).toBe(0);
    await expectAccessible(page, `price-list-delete-group-${String(width)}`);
    await sheet.getByRole("button", { name: "Xóa nhóm", exact: true }).click();

    await expect(sheet).toBeHidden();
    await expect(page.getByRole("heading", { level: 2, name })).toHaveCount(0);
    // A new group is the last on the list: focus goes back to the page's title.
    await expect(page.getByRole("heading", { level: 1, name: "Bảng giá dịch vụ" })).toBeFocused();
    const left = (await (await page.request.get("/api/v1/service-categories")).json()) as { name: string }[];
    expect(left.map((c) => c.name)).not.toContain(name);
  }
});

test("a group with services: no 'Xóa nhóm', and the API refuses (409 catalog.category_in_use)", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto("/bang-gia");
  await page.getByRole("button", { name: "Sửa nhóm Gội đầu dưỡng sinh" }).click();
  const form = page.getByRole("form", { name: "Sửa nhóm: Gội đầu dưỡng sinh" });
  await expect(form.getByRole("button", { name: "Lưu nhóm dịch vụ" })).toBeVisible();
  await expect(form.getByRole("button", { name: /^Xóa nhóm/ })).toHaveCount(0);
  const groups = (await (await page.request.get("/api/v1/service-categories")).json()) as { id: string; name: string }[];
  const used = groups.find((c) => c.name === "Gội đầu dưỡng sinh");
  expect(used, "the seeded menu").toBeDefined();
  const refused = await page.request.delete(`/api/v1/service-categories/${used?.id ?? ""}`, { headers: WRITE });
  expect(refused.status()).toBe(409);
  expect(((await refused.json()) as { code: string }).code).toBe("catalog.category_in_use");
});
