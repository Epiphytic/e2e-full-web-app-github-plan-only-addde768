const fs = require("fs");
const path = require("path");
const jwt = require("jsonwebtoken");

/**
 * Generate a JWT token for testing using the local CA private key.
 * @param {string} sub - Subject (username)
 * @param {number} expiresInSec - Token lifetime in seconds
 * @returns {string} JWT token
 */
function generateToken(sub = "admin", expiresInSec = 60) {
  const certsDir = path.resolve(__dirname, "../../certs");
  const privateKey = fs.readFileSync(path.join(certsDir, "private.pem"));

  const token = jwt.sign(
    { sub },
    privateKey,
    {
      algorithm: "RS256",
      expiresIn: expiresInSec,
      header: { kid: "sqlite-editor-key-1" },
    }
  );

  return token;
}

/**
 * Generate a JWT token that is already expired.
 * @param {string} sub - Subject (username)
 * @returns {string} Expired JWT token
 */
function generateExpiredToken(sub = "admin") {
  const certsDir = path.resolve(__dirname, "../../certs");
  const privateKey = fs.readFileSync(path.join(certsDir, "private.pem"));

  const now = Math.floor(Date.now() / 1000);
  const payload = {
    sub,
    iat: now - 3600,
    exp: now - 60,
  };

  const token = jwt.sign(payload, privateKey, {
    algorithm: "RS256",
    header: { kid: "sqlite-editor-key-1" },
  });

  return token;
}

/**
 * Login to the application via the UI.
 * @param {import('@playwright/test').Page} page
 * @param {string} username
 * @param {string} password
 */
async function loginViaUI(page, username = "admin", password = "admin") {
  await page.goto("/login");
  await page.fill("#username", username);
  await page.fill("#password", password);
  await page.click('button[type="submit"]');
  // htmx will redirect via HX-Redirect header
  await page.waitForURL("**/tables", { timeout: 10000 });
}

/**
 * Login by setting a JWT cookie directly (bypasses UI).
 * @param {import('@playwright/test').BrowserContext} context
 * @param {string} baseURL
 * @param {number} expiresInSec
 */
async function loginViaCookie(context, baseURL = "http://127.0.0.1:8080", expiresInSec = 300) {
  const token = generateToken("admin", expiresInSec);
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
}

module.exports = {
  generateToken,
  generateExpiredToken,
  loginViaUI,
  loginViaCookie,
};
