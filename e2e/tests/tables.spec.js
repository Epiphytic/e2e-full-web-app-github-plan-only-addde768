// @ts-check
const { test, expect } = require("@playwright/test");
const { loginViaUI, loginViaCookie } = require("./helpers");

test.describe("Table Management", () => {
  test.beforeEach(async ({ page, context }) => {
    const baseURL = process.env.BASE_URL || "http://127.0.0.1:8080";
    await loginViaCookie(context, baseURL);
    await page.goto("/tables");
  });

  test("should display the tables page", async ({ page }) => {
    await expect(page.locator("h1")).toContainText("SQLite Editor");
    await expect(page.locator("h2").first()).toContainText("Create New Table");
  });

  test("should create a new table", async ({ page }) => {
    // Fill in table creation form
    await page.fill('input[name="name"]', "test_users");
    await page.fill('input[name="columns"]', "id:INTEGER,name:TEXT,email:TEXT");
    await page.click('button:has-text("Create Table")');

    // Wait for htmx response
    await page.waitForSelector('text=test_users');
    await expect(page.locator("#table-list")).toContainText("test_users");
  });

  test("should remove a table", async ({ page }) => {
    // First create a table to remove
    await page.fill('input[name="name"]', "to_delete");
    await page.fill('input[name="columns"]', "id:INTEGER");
    await page.click('button:has-text("Create Table")');
    await page.waitForSelector('text=to_delete');

    // Accept the confirmation dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Click drop button for the to_delete table
    const dropButton = page.locator('button:has-text("Drop")').filter({ has: page.locator('xpath=ancestor::tr[contains(., "to_delete")]') }).first();
    // Alternative: find the row and click the button within it
    const row = page.locator("tr", { hasText: "to_delete" });
    await row.locator('button:has-text("Drop")').click();

    // Wait for the table to be removed
    await page.waitForFunction(() => !document.body.innerText.includes("to_delete"));
    await expect(page.locator("#table-list")).not.toContainText("to_delete");
  });
});
