// @ts-check
const { test, expect } = require("@playwright/test");
const { generateToken, generateExpiredToken, loginViaUI, loginViaCookie } = require("./helpers");

test.describe("Authentication", () => {
  test("should show login page when not authenticated", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveURL(/\/login/);
    await expect(page.locator("h1")).toContainText("SQLite Editor");
    await expect(page.locator("#username")).toBeVisible();
    await expect(page.locator("#password")).toBeVisible();
  });

  test("should login with valid credentials", async ({ page }) => {
    await loginViaUI(page);
    await expect(page).toHaveURL(/\/tables/);
    await expect(page.locator("h1")).toContainText("SQLite Editor");
  });

  test("should show error for invalid credentials", async ({ page }) => {
    await page.goto("/login");
    await page.fill("#username", "wrong");
    await page.fill("#password", "wrong");
    await page.click('button[type="submit"]');
    // The error response is swapped into #login-error by htmx
    await expect(page.locator("#login-error")).toContainText("Invalid username or password", { timeout: 10000 });
  });

  test("should authenticate with a short-lived JWT token via cookie", async ({ page, context }) => {
    const baseURL = process.env.BASE_URL || "http://127.0.0.1:8080";
    await loginViaCookie(context, baseURL, 300);
    await page.goto("/tables");
    await expect(page).toHaveURL(/\/tables/);
    await expect(page.locator("h1")).toContainText("SQLite Editor");
  });

  test("should reject expired JWT token", async ({ page, context }) => {
    const baseURL = process.env.BASE_URL || "http://127.0.0.1:8080";
    const token = generateExpiredToken("admin");
    const url = new URL(baseURL);
    await context.addCookies([
      {
        name: "token",
        value: token,
        domain: url.hostname,
        path: "/",
        httpOnly: true,
      },
    ]);
    await page.goto("/tables");
    // Should redirect to login since token is expired
    await expect(page).toHaveURL(/\/login/);
  });

  test("should expose .well-known/jwks.json endpoint", async ({ request }) => {
    const response = await request.get("/.well-known/jwks.json");
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body.keys).toBeDefined();
    expect(body.keys.length).toBeGreaterThan(0);
    expect(body.keys[0].kty).toBe("RSA");
    expect(body.keys[0].alg).toBe("RS256");
    expect(body.keys[0].kid).toBe("sqlite-editor-key-1");
    expect(body.keys[0].n).toBeDefined();
    expect(body.keys[0].e).toBeDefined();
  });
});
