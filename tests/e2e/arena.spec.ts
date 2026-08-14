import { expect, test, type Page } from "@playwright/test";

async function installPromotionPosition(page: Page) {
  const fen = "7k/Pp6/8/8/8/8/8/7K w - - 0 1";
  await page.evaluate(async (initialFen) => {
    const db = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open("llm-chess-arena");
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const stored = await new Promise<any>((resolve, reject) => {
      const request = db.transaction("state", "readonly").objectStore("state").get("app");
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const state = stored ?? {
      schema_version: 1,
      profiles: [
        { id: "00000000-0000-4000-8000-000000000001", kind: "human", name: "나", model: "", elo: 1200, active: true },
        { id: "00000000-0000-4000-8000-000000000002", kind: "llm", name: "LLM A", model: "수동 프롬프트 연결", elo: 1200, active: true },
      ],
      games: [],
      ratings: [],
    };
    const human = state.profiles.find((profile: any) => profile.kind === "human");
    const llm = state.profiles.find((profile: any) => profile.kind === "llm");
    state.games = [{
      schema_version: 1,
      id: "00000000-0000-4000-8000-000000000099",
      mode: "human_vs_llm",
      protocol: "arena_direct",
      white: { id: human.id, name: human.name, kind: "human", elo_before: human.elo },
      black: { id: llm.id, name: llm.name, kind: "llm", elo_before: llm.elo },
      rated: false,
      started_at: Date.now(),
      finished_at: null,
      result: null,
      termination: null,
      initial_fen: initialFen,
      current_fen: initialFen,
      moves: [],
      engine: null,
      review: [],
      coaching: null,
    }];
    await new Promise<void>((resolve, reject) => {
      const transaction = db.transaction("state", "readwrite");
      transaction.objectStore("state").put(state, "app");
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error);
    });
    db.close();
  }, fen);
}

test("human and manual LLM game is validated and saved locally", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByRole("heading", { name: "LLM Chess Arena" })).toBeVisible();
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();

  await page.getByRole("button", { name: "대국 시작" }).click();
  await expect(page.locator(".file-label")).toHaveText(["a", "b", "c", "d", "e", "f", "g", "h"]);
  await expect(page.locator(".rank-label")).toHaveText(["8", "7", "6", "5", "4", "3", "2", "1"]);
  const whitePawn = page.getByRole("button", { name: "e2" }).locator(".piece-white");
  const blackPawn = page.getByRole("button", { name: "e7" }).locator(".piece-black");
  await expect(whitePawn).toHaveText("♟");
  await expect(blackPawn).toHaveText("♟");
  await expect(whitePawn).toHaveCSS("color", "rgb(255, 255, 255)");
  await expect(blackPawn).toHaveCSS("color", "rgb(17, 17, 17)");
  await page.getByRole("button", { name: "e2" }).click();
  await page.getByRole("button", { name: "e4" }).click();
  await expect(page.getByRole("button", { name: "e2" })).toHaveClass(/last-from/);
  await expect(page.getByRole("button", { name: "e4" })).toHaveClass(/last-to/);
  await expect(page.locator(".last-move-info")).toContainText("e2 → e4");
  await expect(page.getByRole("heading", { name: "LLM 수 입력" })).toBeVisible();

  await page.getByPlaceholder(/LLM 응답/).fill('{"move":"e7e5"}');
  await page.getByRole("button", { name: "응답 검증 후 두기" }).click();
  await expect(page.getByRole("button", { name: "e7" })).toHaveClass(/last-from/);
  await expect(page.getByRole("button", { name: "e5" })).toHaveClass(/last-to/);
  await expect(page.getByRole("button", { name: "e2" })).not.toHaveClass(/last-from/);
  await expect(page.locator(".last-move-info")).toContainText("e7 → e5");
  await expect(page.locator(".moves span")).toHaveText(["1. e4", "2. e5"]);
  await page.getByRole("button", { name: "현재 포지션 분석" }).click();
  await expect(page.locator(".analysis-result")).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "무승부" }).click();
  await expect(page.getByRole("heading", { name: /종료 · 1\/2-1\/2/ })).toBeVisible();

  await expect.poll(async () => page.evaluate(async () =>
    (await indexedDB.databases()).some((database) => database.name === "llm-chess-arena")
  )).toBe(true);

  await page.getByRole("button", { name: "기록", exact: true }).click();
  await page.getByRole("button", { name: "Stockfish 리뷰" }).click();
  await expect(page.locator(".review-board")).toBeVisible();
  await expect(page.getByLabel("리뷰 e2")).toHaveClass(/actual-from/);
  await expect(page.getByLabel("리뷰 e4")).toHaveClass(/actual-to/);
  await expect(page.locator(".recommendation b")).toHaveText(/[a-h][1-8] → [a-h][1-8]/, { timeout: 30_000 });
  await expect(page.locator(".review-board .best-from")).toHaveCount(1);
  await expect(page.locator(".review-board .best-to")).toHaveCount(1);

  const next = page.getByRole("button", { name: "다음 수" });
  await expect(next).toBeEnabled({ timeout: 30_000 });
  await next.click();
  await expect(page.getByLabel("리뷰 e7")).toHaveClass(/actual-from/);
  await expect(page.getByLabel("리뷰 e5")).toHaveClass(/actual-to/);
  await page.getByRole("button", { name: "이전 수" }).click();
  await expect(page.getByLabel("리뷰 e2")).toHaveClass(/actual-from/);
});

test("Stockfish 18 worker automatically plays without showing analysis", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();
  await page.getByLabel("대국 유형").selectOption("hvs");
  await page.getByLabel("주 선수 색").selectOption("black");
  await page.getByRole("button", { name: "대국 시작" }).click();

  await expect(page.getByRole("heading", { name: "흑 차례" })).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".moves span")).toHaveCount(1);
  await expect(page.locator(".review-list")).toHaveCount(0);
});

test("human can drag a piece to make a legal move", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();
  await page.getByRole("button", { name: "대국 시작" }).click();

  const source = page.getByRole("button", { name: "e2" });
  const target = page.getByRole("button", { name: "e4" });
  await expect(source).toHaveAttribute("draggable", "true");
  await expect(page.getByRole("button", { name: "e7" })).toHaveAttribute("draggable", "false");
  await source.dragTo(target);

  await expect(target.locator(".piece-white")).toHaveText("♟");
  await expect(source.locator(".piece")).toBeEmpty();
  await expect(source).toHaveClass(/last-from/);
  await expect(target).toHaveClass(/last-to/);
  await expect(page.getByRole("heading", { name: "LLM 수 입력" })).toBeVisible();
  await expect(page.locator(".moves span")).toHaveText(["1. e4"]);
});

test("human chooses the promotion piece before the move is committed", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();
  await installPromotionPosition(page);
  await page.reload();
  await expect(page.getByRole("heading", { name: "백 차례" })).toBeVisible();

  await page.getByRole("button", { name: "a7" }).click();
  await page.getByRole("button", { name: "a8" }).click();
  await expect(page.getByRole("heading", { name: "승격할 기물을 선택하세요" })).toBeVisible();
  for (const piece of ["퀸", "룩", "비숍", "나이트"]) {
    const action = piece === "나이트" ? "나이트로 승격" : `${piece}으로 승격`;
    await expect(page.getByRole("button", { name: action })).toBeVisible();
  }
  await expect(page.getByRole("button", { name: "a7" }).locator(".piece-white")).toHaveText("♟");
  await expect(page.getByRole("button", { name: "a8" }).locator(".piece")).toBeEmpty();
  await expect(page.locator(".moves span")).toHaveCount(0);

  await page.getByRole("button", { name: "나이트로 승격" }).click();
  await expect(page.getByRole("button", { name: "a8" }).locator(".piece-white")).toHaveText("♞");
  await expect(page.locator(".last-move-info")).toContainText("a7 → a8");
  await expect(page.locator(".moves span")).toContainText("a8=N");
  await expect(page.getByRole("heading", { name: "LLM 수 입력" })).toBeVisible();
});

test("a named LLM profile can be added and selected from match setup", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();
  await page.getByLabel("새 LLM 이름").fill("Opus 5 Extra Thinking");
  await page.getByRole("button", { name: "추가·선택" }).click();

  await expect(page.getByLabel("LLM 이름/프로필")).toContainText("Opus 5 Extra Thinking");
  await expect(page.getByText("LLM 프로필 ‘Opus 5 Extra Thinking’을 선택했습니다.")).toBeVisible();
  await page.getByRole("button", { name: "대국 시작" }).click();
  await expect(page.locator(".player.black b")).toHaveText("Opus 5 Extra Thinking");
});

test("white, black, and random color choices assign both players", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByText("IndexedDB에 자동 저장됩니다.")).toBeVisible();

  const side = page.getByLabel("주 선수 색");
  await expect(side.locator("option")).toHaveText(["백", "흑", "랜덤"]);
  await side.selectOption("random");
  await page.getByRole("button", { name: "대국 시작" }).click();

  const white = await page.locator(".player.white b").textContent();
  const black = await page.locator(".player.black b").textContent();
  expect(new Set([white, black])).toEqual(new Set(["나", "LLM A"]));
  await expect(page.getByText(/색상 배정 · 백: .+ · 흑: .+/)).toBeVisible();
});
