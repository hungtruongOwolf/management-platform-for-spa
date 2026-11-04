import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { createHmac, randomInt } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { runFile } from "../stack";

/** The spa and owner account the setup project provisioned for this run. */
export interface RunCredentials {
  slug: string;
  phone: string;
  password: string;
  fullName: string;
  /** Owner's TOTP secret (base32), captured from the enrollment screen on the first sign-in. */
  totpKey?: string;
  /** Last TOTP time step used: the server accepts each step once (replay guard). */
  lastTotpStep?: number;
  /** Manager account (no MFA): used by Lighthouse to audit the front desk `/quay`. */
  managerPhone?: string;
  managerPassword?: string;
}

export function credentials(): RunCredentials {
  return JSON.parse(readFileSync(runFile, "utf8")) as RunCredentials;
}

function saveCredentials(run: RunCredentials): void {
  writeFileSync(runFile, JSON.stringify(run, null, 2));
}

function base32Decode(input: string): Buffer {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const char of input.replace(/[\s=]/g, "").toUpperCase()) {
    const index = alphabet.indexOf(char);
    if (index < 0) throw new Error(`not base32: ${char}`);
    value = (value << 5) | index;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(out);
}

/** RFC 6238 code (HMAC-SHA-1, 6 digits) for one 30 s time step, as an authenticator app shows it. */
function totpAt(key: string, step: number): string {
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(step));
  const mac = createHmac("sha1", base32Decode(key)).update(counter).digest();
  const offset = (mac[mac.length - 1] ?? 0) & 0x0f;
  const binary = mac.readUInt32BE(offset) & 0x7fffffff;
  return String(binary % 1_000_000).padStart(6, "0");
}

/**
 * A code for a time step the server has not seen yet. The server allows ±1 step, so a step one
 * ahead is still valid; beyond that we wait. Never sends a stale code (that would count toward
 * the account lockout).
 */
async function nextTotpCode(): Promise<string> {
  const run = credentials();
  if (!run.totpKey) throw new Error("owner has no TOTP key yet (setup enrolls it)");
  const now = Math.floor(Date.now() / 30_000);
  const step = Math.max(now, (run.lastTotpStep ?? 0) + 1);
  if (step > now + 1) {
    await new Promise((resolve) => setTimeout(resolve, (step - 1) * 30_000 - Date.now() + 500));
  }
  saveCredentials({ ...run, lastTotpStep: step });
  return totpAt(run.totpKey, step);
}

/** Second login step in the browser: enrolls on the first sign-in, enters a code afterwards. */
async function completeMfa(page: Page): Promise<void> {
  const enroll = page.getByRole("heading", { name: "Thiết lập xác thực hai bước" });
  const verify = page.getByRole("heading", { name: "Xác thực hai bước" });
  await expect(enroll.or(verify)).toBeVisible();
  if (await enroll.isVisible()) {
    const key = (await page.getByTestId("manual-key").textContent())?.replace(/\s/g, "") ?? "";
    saveCredentials({ ...credentials(), totpKey: key });
    await page.getByLabel("Mã xác thực (6 số)").fill(await nextTotpCode());
    await page.getByRole("button", { name: "Bật xác thực hai bước" }).click();
    await expect(page.getByRole("list", { name: "Mã khôi phục" }).getByRole("listitem")).toHaveCount(10);
    await page.getByLabel("Tôi đã lưu các mã khôi phục").check();
    await page.getByRole("button", { name: "Tiếp tục" }).click();
  } else {
    await page.getByLabel("Mã xác thực (6 số)").fill(await nextTotpCode());
    await page.getByRole("button", { name: "Xác nhận" }).click();
  }
  await expect(page).not.toHaveURL(/\/xac-thuc/);
}

/** Fills the login form already on screen, then the MFA step if the server asks for it. */
export async function submitLogin(page: Page): Promise<void> {
  const { phone, password } = credentials();
  await page.getByLabel("Số điện thoại").fill(phone);
  await page.getByLabel("Mật khẩu").fill(password);
  await page.getByRole("button", { name: "Đăng nhập" }).click();
  await expect(page).not.toHaveURL(/\/dang-nhap/);
  if (new URL(page.url()).pathname === "/xac-thuc") await completeMfa(page);
}

/** Signs in through the real login form (cookie set by the API, same as a user). */
export async function signIn(page: Page, path = "/dang-nhap"): Promise<void> {
  await page.goto(path);
  await submitLogin(page);
}

/** API-level owner sign-in (password + TOTP), returning the full-session cookie pair. */
export async function apiSignIn(
  post: (path: string, data: unknown, cookie?: string) => Promise<{ status: number; cookie: string; body: unknown }>,
): Promise<string> {
  const { phone, password } = credentials();
  const login = await post("/api/v1/auth/login", { phone, password });
  expect(login.status).toBe(200);
  if ((login.body as { next?: string }).next === "done") return login.cookie;
  const verify = await post("/api/v1/auth/mfa/verify", { code: await nextTotpCode() }, login.cookie);
  expect(verify.status).toBe(200);
  return verify.cookie;
}

/** A customer phone unique within the run (retries never collide on the uniqueness rule). */
export function uniquePhone(): string {
  return `0912${String(randomInt(0, 1_000_000)).padStart(6, "0")}`;
}

const WCAG = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

/** Spec §6: WCAG 2.2 AA. Serious/critical violations fail; lesser ones are attached for triage. */
export async function expectAccessible(
  page: Page,
  screen: string,
): Promise<void> {
  const results = await new AxeBuilder({ page }).withTags(WCAG).analyze();
  const blocking = results.violations.filter(
    (v) => v.impact === "serious" || v.impact === "critical",
  );
  const minor = results.violations.filter((v) => !blocking.includes(v));
  if (minor.length > 0) {
    await test.info().attach(`axe-${screen}-minor.json`, {
      body: JSON.stringify(minor, null, 2),
      contentType: "application/json",
    });
  }
  const summary = blocking.map(
    (v) =>
      `${v.impact ?? "?"} ${v.id}: ${v.help}\n    ${v.nodes.map((n) => n.target.join(" ")).join("\n    ")}`,
  );
  expect(summary, `axe violations on ${screen}`).toEqual([]);
}
