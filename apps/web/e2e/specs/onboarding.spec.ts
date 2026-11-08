import { expect, test } from "@playwright/test";
import { ownerState } from "../stack";
import { credentials, expectAccessible, nextTotpCode } from "../support/run";

/**
 * ADR-0016 (EM 2026-11-08): a spa onboards its people in the app. The owner adds a manager with a
 * login from "Hồ sơ nhân viên", confirms their own password, hands over the temporary password
 * shown once; the manager signs in, can do nothing but set their own password, then works.
 */
test("the owner creates a manager's login; the manager is forced to set a password, then works", async ({ browser }) => {
  test.setTimeout(120_000);
  const name = `Quản lý Mới ${String(Date.now()).slice(-5)}`;
  const phone = `0937${String(Date.now()).slice(-6)}`;
  const newPassword = "mot-cau-de-nho-cua-toi";

  // The owner: add the profile with "Tạo tài khoản đăng nhập ngay".
  const ownerContext = await browser.newContext({ storageState: ownerState });
  const owner = await ownerContext.newPage();
  await owner.goto("/nhan-vien?tab=profiles");
  await owner.getByRole("button", { name: "Thêm nhân viên" }).click();
  const form = owner.getByRole("dialog", { name: "Thêm nhân viên" });
  await form.getByLabel("Họ tên").fill(name);
  await form.getByRole("checkbox", { name: "Tạo tài khoản đăng nhập ngay" }).check();
  await form.getByLabel("Số điện thoại đăng nhập").fill(phone);
  await form.getByLabel("Vai trò").selectOption("manager");
  await form.getByRole("button", { name: "Lưu hồ sơ" }).click();

  // The owner's session is older than the re-auth window: password + code again, then retried.
  const again = owner.getByRole("dialog", { name: "Xác nhận lại mật khẩu" });
  const shown = owner.getByRole("dialog", { name: `Mật khẩu tạm của ${name}` });
  await expect(again.or(shown)).toBeVisible();
  if (await again.isVisible()) {
    await expectAccessible(owner, "reauth-dialog");
    await again.getByLabel("Mật khẩu của bạn").fill(credentials().password);
    await again.getByLabel(/Mã xác thực/).fill(await nextTotpCode());
    await again.getByRole("button", { name: "Xác nhận" }).click();
  }
  await expect(shown).toBeVisible();
  const temporary = ((await shown.getByLabel("Mật khẩu tạm").textContent()) ?? "").trim();
  expect(temporary).toMatch(/^[A-Z2-9]{4}-[A-Z2-9]{4}-[A-Z2-9]{4}$/);
  await expectAccessible(owner, "temporary-password");
  await expect(shown.getByRole("button", { name: "Xong" })).toBeDisabled();
  await shown.getByRole("checkbox", { name: "Tôi đã đưa mật khẩu cho nhân viên" }).check();
  await shown.getByRole("button", { name: "Xong" }).click();
  const sheet = owner.getByRole("dialog", { name });
  await expect(sheet.getByText("Quản lý", { exact: true }).first()).toBeVisible();
  await ownerContext.close();

  // The manager: the temporary password opens only "Đặt mật khẩu mới".
  const managerContext = await browser.newContext();
  const manager = await managerContext.newPage();
  await manager.goto("/dang-nhap");
  await manager.getByLabel("Số điện thoại").fill(phone);
  await manager.getByLabel("Mật khẩu").fill(temporary);
  await manager.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(manager).toHaveURL(/\/doi-mat-khau/);
  await expect(manager.getByRole("heading", { level: 1, name: "Đặt mật khẩu mới" })).toBeVisible();
  await expectAccessible(manager, "password-change");
  const blocked = await manager.request.get("/api/v1/me");
  expect(blocked.status()).toBe(403);
  await manager.goto("/nhan-vien");
  await expect(manager).toHaveURL(/\/doi-mat-khau/);
  await manager.getByLabel("Mật khẩu tạm").fill(temporary);
  await manager.getByLabel("Mật khẩu mới", { exact: true }).fill(newPassword);
  await manager.getByLabel("Nhập lại mật khẩu mới").fill(newPassword);
  await manager.getByRole("button", { name: "Đổi mật khẩu" }).click();
  await expect(manager).not.toHaveURL(/\/doi-mat-khau/);

  // A working manager, linked to their own profile: the books open and can be written.
  await manager.goto("/nhan-vien");
  await expect(manager.getByRole("tab", { name: "Sổ hằng ngày" })).toBeVisible();
  await expect(manager.getByText(/Tài khoản của bạn chưa gắn với hồ sơ nhân viên/)).toHaveCount(0);
  expect((await manager.request.get("/api/v1/me/staff")).status()).toBe(200);
  await managerContext.close();
});
