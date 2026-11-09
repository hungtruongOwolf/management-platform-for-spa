import { expect, test, type Page } from "@playwright/test";
import { credentials, expectAccessible, totpAt } from "../support/run";

/**
 * ADR-0018, spec platform-admin US1/US3/US5: the operator signs in to the console (password +
 * TOTP), creates a spa with its first owner, the owner signs in with the temporary password,
 * enrolls MFA and must set a password; the operator switches "Lịch hẹn" off and the owner can no
 * longer see or use it.
 */

/** Codes for a key, never reusing a time step (the server accepts each step once). */
function authenticator(key: string) {
  let last = 0;
  return async (): Promise<string> => {
    const now = Math.floor(Date.now() / 30_000);
    const step = Math.max(now, last + 1);
    if (step > now + 1) await new Promise((r) => setTimeout(r, (step - 1) * 30_000 - Date.now() + 500));
    last = step;
    return totpAt(key, step);
  };
}

/** The console's step-up dialog, if the 5-minute window has passed. */
async function stepUpIfAsked(page: Page, password: string, code: () => Promise<string>, done: ReturnType<Page["getByRole"]>) {
  const dialog = page.getByRole("dialog", { name: "Xác nhận lại" });
  await expect(dialog.or(done)).toBeVisible();
  if (await dialog.isVisible()) {
    await dialog.getByLabel("Mật khẩu").fill(password);
    await dialog.getByLabel("Mã xác thực").fill(await code());
    await dialog.getByRole("button", { name: "Xác nhận" }).click();
  }
}

test("operator creates a spa and its owner, then switches a feature off for it", async ({ browser }) => {
  test.setTimeout(180_000);
  const { operatorUsername, operatorPassword } = credentials();
  if (!operatorUsername || !operatorPassword) throw new Error("setup did not create the operator");
  const stamp = String(Date.now()).slice(-6);
  const ownerPhone = `0938${stamp}`;
  const spaName = `Spa Admin ${stamp}`;
  const ownerPassword = "mat-khau-rieng-cua-chu-spa";

  // The operator: the console has its own sign-in, always with a second factor.
  const opContext = await browser.newContext();
  const op = await opContext.newPage();
  await op.goto("/admin");
  await expect(op).toHaveURL(/\/admin\/dang-nhap/);
  await expectAccessible(op, "admin-login");
  await op.getByLabel("Tên đăng nhập").fill(operatorUsername);
  await op.getByLabel("Mật khẩu").fill(operatorPassword);
  await op.getByRole("button", { name: "Tiếp tục" }).click();
  await expect(op.getByRole("heading", { name: "Thiết lập xác thực hai bước" })).toBeVisible();
  const opKey = ((await op.getByTestId("admin-manual-key").textContent()) ?? "").trim();
  const opCode = authenticator(opKey);
  await op.getByLabel("Mã xác thực").fill(await opCode());
  await op.getByRole("button", { name: "Xác nhận" }).click();
  await expect(op.getByRole("list", { name: "Mã khôi phục" }).getByRole("listitem")).toHaveCount(10);
  await op.getByRole("button", { name: "Tôi đã lưu, vào trang quản trị" }).click();
  await expect(op.getByRole("heading", { level: 1, name: "Spa" })).toBeVisible();
  await expectAccessible(op, "admin-tenants");

  // Create the spa and its owner; the temporary password shows once.
  await op.getByRole("button", { name: "Tạo spa" }).click();
  const form = op.getByRole("dialog", { name: "Tạo spa" });
  await form.getByLabel("Tên spa").fill(spaName);
  await form.getByLabel("Tên chủ spa").fill("Chủ Spa Admin");
  await form.getByLabel("Số điện thoại chủ spa").fill(ownerPhone);
  await form.getByRole("button", { name: "Tạo spa" }).click();
  const created = op.getByRole("dialog", { name: "Đã tạo spa" });
  await stepUpIfAsked(op, operatorPassword, opCode, created);
  await expect(created).toBeVisible();
  const temporary = ((await created.getByLabel("Mật khẩu tạm").textContent()) ?? "").trim();
  expect(temporary).toMatch(/^[A-Z2-9]{4}-[A-Z2-9]{4}-[A-Z2-9]{4}$/);
  await created.getByRole("button", { name: "Tôi đã lưu mật khẩu" }).click();

  // The owner: temporary password → MFA enrollment → must set a password → the app.
  const ownerContext = await browser.newContext();
  const owner = await ownerContext.newPage();
  await owner.goto("/dang-nhap");
  await owner.getByLabel("Số điện thoại").fill(ownerPhone);
  await owner.getByLabel("Mật khẩu").fill(temporary);
  await owner.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(owner.getByRole("heading", { name: "Thiết lập xác thực hai bước" })).toBeVisible();
  const ownerKey = ((await owner.getByTestId("manual-key").textContent()) ?? "").replace(/\s/g, "");
  const ownerCode = authenticator(ownerKey);
  await owner.getByLabel("Mã xác thực (6 số)").fill(await ownerCode());
  await owner.getByRole("button", { name: "Bật xác thực hai bước" }).click();
  await expect(owner.getByRole("list", { name: "Mã khôi phục" }).getByRole("listitem")).toHaveCount(10);
  await owner.getByLabel("Tôi đã lưu các mã khôi phục").check();
  await owner.getByRole("button", { name: "Tiếp tục" }).click();
  await expect(owner).toHaveURL(/\/doi-mat-khau/);
  expect((await owner.request.get("/api/v1/me")).status()).toBe(403);
  await owner.getByLabel("Mật khẩu tạm").fill(temporary);
  await owner.getByLabel("Mật khẩu mới", { exact: true }).fill(ownerPassword);
  await owner.getByLabel("Nhập lại mật khẩu mới").fill(ownerPassword);
  await owner.getByRole("button", { name: "Đổi mật khẩu" }).click();
  await expect(owner).not.toHaveURL(/\/doi-mat-khau/);
  const mainNav = owner.getByRole("navigation", { name: "Chính" });
  await expect(mainNav.getByRole("link", { name: "Lịch hẹn" })).toBeVisible();

  // The operator switches "Lịch hẹn" off for this spa only.
  await op.getByRole("link", { name: spaName }).click();
  await op.getByRole("button", { name: "Tắt Lịch hẹn" }).click();
  const sheet = op.getByRole("dialog", { name: "Tắt Lịch hẹn?" });
  await sheet.getByLabel("Lý do").fill("Gói cơ bản, chưa có lịch hẹn");
  await sheet.getByRole("button", { name: "Tắt tính năng" }).click();
  const switched = op.getByRole("button", { name: "Bật Lịch hẹn" });
  await stepUpIfAsked(op, operatorPassword, opCode, switched);
  await expect(switched).toBeVisible();
  await expectAccessible(op, "admin-tenant");

  // The owner's next request: no "Lịch hẹn" anywhere, and the API refuses it.
  await owner.goto("/");
  await expect(mainNav.getByRole("link", { name: "Ghi lượt khách" })).toBeVisible();
  await expect(mainNav.getByRole("link", { name: "Lịch hẹn" })).toHaveCount(0);
  const refused = await owner.request.get("/api/v1/appointments");
  expect(refused.status()).toBe(403);
  expect(((await refused.json()) as { code: string }).code).toBe("feature.disabled");

  await ownerContext.close();
  await opContext.close();
});
