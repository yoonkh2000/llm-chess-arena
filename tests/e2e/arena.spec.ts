import { expect, test } from "@playwright/test";

test("human and manual LLM game is validated and saved locally", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByRole("heading", { name: "LLM Chess Arena" })).toBeVisible();
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();

  await page.getByRole("button", { name: "대국 시작" }).click();
  await page.getByRole("button", { name: "e2" }).click();
  await page.getByRole("button", { name: "e4" }).click();
  await expect(page.getByRole("heading", { name: "LLM 수 입력" })).toBeVisible();

  await page.getByPlaceholder(/LLM 응답/).fill('{"move":"e7e5"}');
  await page.getByRole("button", { name: "응답 검증 후 두기" }).click();
  await expect(page.getByText(/e5/)).toBeVisible();
  await page.getByRole("button", { name: "무승부" }).click();
  await expect(page.getByRole("heading", { name: /종료 · 1\/2-1\/2/ })).toBeVisible();

  await expect.poll(async () => page.evaluate(async () =>
    (await indexedDB.databases()).some((database) => database.name === "llm-chess-arena")
  )).toBe(true);
});

test("Stockfish 18 worker automatically plays without showing analysis", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();
  await page.getByLabel("대국 유형").selectOption("hvs");
  await page.getByLabel("주 프로필 색").selectOption("black");
  await page.getByRole("button", { name: "대국 시작" }).click();

  await expect(page.getByRole("heading", { name: "흑 차례" })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".moves span")).toHaveCount(1);
  await expect(page.locator(".review-list")).toHaveCount(0);
});
