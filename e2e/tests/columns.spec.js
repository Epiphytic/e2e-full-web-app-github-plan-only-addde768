// @ts-check
const { test, expect } = require("@playwright/test");
const { loginViaCookie } = require("./helpers");

test.describe("Table Structure (Columns)", () => {
  test.beforeEach(async ({ page, context }) => {
    const baseURL = process.env.BASE_URL || "http://127.0.0.1:8080";
    await loginViaCookie(context, baseURL);

    // Create a test table via the API
    await page.goto("/tables");
    await page.fill('input[name="name"]', "column_test");
    await page.fill('input[name="columns"]', "id:INTEGER,name:TEXT");
    await page.click('button:has-text("Create Table")');
    await page.waitForSelector('text=column_test');

    // Navigate to table detail page
    await page.click('a[href="/tables/column_test"]');
    await page.waitForURL("**/tables/column_test");
  });

  test("should display table columns", async ({ page }) => {
    await expect(page.locator("h1")).toContainText("column_test");
    await expect(page.locator("#table-detail")).toContainText("id");
    await expect(page.locator("#table-detail")).toContainText("name");
    await expect(page.locator("#table-detail")).toContainText("INTEGER");
    await expect(page.locator("#table-detail")).toContainText("TEXT");
  });

  test("should add a new column", async ({ page }) => {
    // Fill in add column form
    await page.fill('input[name="name"]', "age");
    await page.selectOption('select[name="col_type"]', "INTEGER");
    await page.click('button:has-text("Add Column")');

    // Wait for the column to appear
    await page.waitForSelector('text=age');
    await expect(page.locator("#table-detail")).toContainText("age");
  });

  test("should remove a column", async ({ page }) => {
    // First add a column to remove
    await page.fill('input[name="name"]', "temp_col");
    await page.selectOption('select[name="col_type"]', "TEXT");
    await page.click('button:has-text("Add Column")');
    await page.waitForSelector('text=temp_col');

    // Accept the confirmation dialog
    page.on("dialog", (dialog) => dialog.accept());

    // Find the row with temp_col and click the Drop button
    const row = page.locator("tr", { hasText: "temp_col" });
    await row.locator('button:has-text("Drop")').click();

    // Wait for the column to be removed
    await page.waitForFunction(() => !document.body.innerText.includes("temp_col"));
    await expect(page.locator("#table-detail")).not.toContainText("temp_col");
  });
});
