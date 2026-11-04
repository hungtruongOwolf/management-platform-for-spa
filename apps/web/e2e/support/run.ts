import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { randomInt } from "node:crypto";
import { readFileSync } from "node:fs";
import { runFile } from "../stack";

/** The spa and owner account the setup project provisioned for this run. */
export interface RunCredentials {
  slug: string;
  phone: string;
  password: string;
  fullName: string;
}

export function credentials(): RunCredentials {
  return JSON.parse(readFileSync(runFile, "utf8")) as RunCredentials;
}

/** Signs in through the real login form (cookie set by the API, same as a user). */
export async function signIn(page: Page, path = "/dang-nhap"): Promise<void> {
  const { phone, password } = credentials();
  await page.goto(path);
  await page.getByLabel("Số điện thoại").fill(phone);
  await page.getByLabel("Mật khẩu").fill(password);
  await page.getByRole("button", { name: "Đăng nhập" }).click();
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
