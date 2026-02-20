# SQLite Web Editor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a web application that allows authenticated users to edit an SQLite database through a browser UI, with htmx-driven interactivity.

**Architecture:** A Rust backend (actix-web) serves an htmx-based frontend and exposes REST endpoints for database manipulation. JWT authentication uses a local RSA key pair with a `.well-known/jwks.json` endpoint. SQLite is the data store, accessed via rusqlite. Playwright E2E tests validate all user flows. GitHub Actions run Super-Linter and dependency-review on PRs.

**Tech Stack:** Rust (actix-web, rusqlite, jsonwebtoken, tera templates), htmx, Playwright (TypeScript), GitHub Actions

---

## Overview

The application is structured as a monolithic Rust web server that:

1. **Authenticates** users via JWT tokens signed with RS256. A local CA (RSA key pair) is generated at build/setup time. The server exposes `/.well-known/jwks.json` for public key discovery.
2. **Serves** an htmx-based UI using Tera HTML templates. The UI enables users to list tables, create/drop tables, and alter table structure (add/remove columns).
3. **Manages** an SQLite database via rusqlite, executing DDL statements (CREATE TABLE, DROP TABLE, ALTER TABLE) and querying `sqlite_master` for schema introspection.
4. **Tests** all flows end-to-end with Playwright, including JWT login, table CRUD, and column manipulation.
5. **Lints** and reviews via GitHub Actions using Super-Linter and dependency-review-action.

### Directory Structure (Target)

```
.
├── .github/
│   └── workflows/
│       ├── lint.yml
│       ├── dependency-review.yml
│       └── e2e-tests.yml
├── .gitignore
├── Cargo.toml
├── Cargo.lock
├── certs/
│   └── README.md              # Instructions only; keys generated at runtime
├── src/
│   ├── main.rs                # Server entrypoint
│   ├── config.rs              # Configuration loading
│   ├── auth/
│   │   ├── mod.rs             # Auth module
│   │   ├── jwt.rs             # JWT validation, JWKS generation
│   │   ├── keys.rs            # RSA key pair generation/loading
│   │   └── middleware.rs      # Actix auth middleware
│   ├── db/
│   │   ├── mod.rs             # DB module
│   │   ├── connection.rs      # SQLite connection pool
│   │   ├── schema.rs          # Schema introspection queries
│   │   └── operations.rs      # DDL operations (create/drop/alter)
│   ├── handlers/
│   │   ├── mod.rs             # Handler module
│   │   ├── well_known.rs      # .well-known/jwks.json endpoint
│   │   ├── auth.rs            # Login page & token verification
│   │   ├── tables.rs          # Table CRUD handlers
│   │   └── columns.rs         # Column management handlers
│   └── templates/
│       ├── base.html          # Base layout with htmx script
│       ├── login.html         # Login form
│       ├── tables.html        # Table list view
│       ├── table_row.html     # Partial: single table row (htmx swap)
│       ├── columns.html       # Column list for a table
│       └── column_row.html    # Partial: single column row (htmx swap)
├── tests/
│   └── e2e/
│       ├── package.json
│       ├── tsconfig.json
│       ├── playwright.config.ts
│       ├── helpers/
│       │   └── jwt.ts         # JWT token generation for tests
│       └── specs/
│           ├── auth.spec.ts   # Login/auth tests
│           ├── tables.spec.ts # Table CRUD tests
│           └── columns.spec.ts # Column management tests
└── docs/
    └── plans/
        └── (this file)
```

---

## Risk Areas

1. **SQLite DDL limitations** - SQLite does not support `DROP COLUMN` before version 3.35.0. Need to verify rusqlite bundles a recent enough SQLite version, or implement column removal via table recreation.
2. **JWT key management** - RSA key generation at startup must be deterministic for testing (or tests must generate their own tokens with the same key). Plan uses a shared key file that tests can read.
3. **htmx partial rendering** - Must return correct HTML fragments (not full pages) for htmx swap targets. Incorrect content-type or missing `hx-target` can break UI.
4. **SQLite concurrency** - SQLite has limited write concurrency. Need WAL mode and a connection pool with a single writer.
5. **Playwright server startup** - E2E tests need the Rust server running. Must handle startup/teardown cleanly in CI.
6. **Super-Linter configuration** - Super-Linter enables ALL linters by default. Must selectively enable only relevant ones (Rust/clippy, HTML, YAML, TypeScript) to avoid false positives.
7. **Column type handling** - SQLite is loosely typed. Need to decide on a fixed set of column types to present in the UI.

---

## Tasks

### Task CRUISE-001: Project Scaffolding and .gitignore

**Files:**
- Create: `.gitignore`
- Create: `Cargo.toml`
- Create: `src/main.rs` (minimal)

**Step 1: Create .gitignore**

```gitignore
# Rust build artifacts
/target/
**/*.rs.bk
*.pdb

# Dependencies
Cargo.lock is committed for binaries
# But node_modules is not
node_modules/

# Keys and credentials
*.pem
*.key
*.crt
*.p12
*.pfx
certs/*.pem
certs/*.key
certs/*.crt
.env
.env.*
!.env.example

# IDE / Editor files
.idea/
.vscode/
*.swp
*.swo
*~
.project
.classpath
.settings/
*.sublime-project
*.sublime-workspace
.vim/
tags

# OS files
.DS_Store
.DS_Store?
._*
.Spotlight-V100
.Trashes
ehthumbs.db
Thumbs.db
Desktop.ini
$RECYCLE.BIN/

# Log files
*.log
logs/

# Test artifacts
test-results/
playwright-report/
blob-report/

# Build artifacts
dist/
build/
out/

# Temporary files
*.tmp
*.temp
*.bak
*.orig

# Fork-join directories
.fork-join/
```

**Step 2: Create Cargo.toml**

```toml
[package]
name = "sqlite-web-editor"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
actix-files = "0.6"
actix-rt = "2"
rusqlite = { version = "0.31", features = ["bundled"] }
jsonwebtoken = "9"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tera = "1"
base64 = "0.22"
rsa = { version = "0.9", features = ["pem"] }
rand = "0.8"
tokio = { version = "1", features = ["full"] }
env_logger = "0.11"
log = "0.4"
```

**Step 3: Create minimal src/main.rs**

```rust
use actix_web::{web, App, HttpServer, HttpResponse};

async fn health() -> HttpResponse {
    HttpResponse::Ok().body("OK")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    HttpServer::new(|| {
        App::new()
            .route("/health", web::get().to(health))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Successful compilation

**Step 5: Commit**

```bash
git add .gitignore Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: scaffold Rust project with actix-web"
```

---

### Task CRUISE-002: RSA Key Generation and JWKS Endpoint

**Files:**
- Create: `src/auth/mod.rs`
- Create: `src/auth/keys.rs`
- Create: `src/auth/jwt.rs`
- Create: `src/handlers/mod.rs`
- Create: `src/handlers/well_known.rs`
- Modify: `src/main.rs`
- Create: `certs/README.md`

**Step 1: Write a test for RSA key pair generation**

In `src/auth/keys.rs`:

```rust
use rsa::{RsaPrivateKey, RsaPublicKey, pkcs1::EncodeRsaPublicKey, pkcs8::EncodePrivateKey};
use std::path::Path;
use std::fs;

const KEY_SIZE: usize = 2048;

pub struct KeyPair {
    pub private_key: RsaPrivateKey,
    pub public_key: RsaPublicKey,
}

/// Load existing keys from disk or generate new ones.
/// Keys are stored in PEM format at `{dir}/private.pem` and `{dir}/public.pem`.
pub fn load_or_generate_keys(dir: &Path) -> KeyPair {
    let priv_path = dir.join("private.pem");
    let pub_path = dir.join("public.pem");

    if priv_path.exists() && pub_path.exists() {
        let priv_pem = fs::read_to_string(&priv_path).expect("read private key");
        let pub_pem = fs::read_to_string(&pub_path).expect("read public key");
        let private_key = rsa::pkcs8::DecodePrivateKey::from_pkcs8_pem(&priv_pem)
            .expect("parse private key");
        let public_key = rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_pem(&pub_pem)
            .expect("parse public key");
        return KeyPair { private_key, public_key };
    }

    let mut rng = rand::thread_rng();
    let private_key = RsaPrivateKey::new(&mut rng, KEY_SIZE).expect("generate RSA key");
    let public_key = RsaPublicKey::from(&private_key);

    fs::create_dir_all(dir).expect("create certs dir");
    fs::write(&priv_path, private_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).expect("encode private key").as_bytes())
        .expect("write private key");
    fs::write(&pub_path, public_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).expect("encode public key"))
        .expect("write public key");

    KeyPair { private_key, public_key }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_and_reload_keys() {
        let dir = tempdir().unwrap();
        let kp1 = load_or_generate_keys(dir.path());
        let kp2 = load_or_generate_keys(dir.path());
        // Reloaded keys should match
        assert_eq!(
            kp1.public_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).unwrap(),
            kp2.public_key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).unwrap()
        );
    }
}
```

**Step 2: Run test to verify it passes**

Run: `cargo test --lib auth::keys::tests::test_generate_and_reload_keys`
Expected: PASS

**Step 3: Write JWKS endpoint**

In `src/auth/jwt.rs`:

```rust
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm, TokenData};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn validate_token(token: &str, public_key_pem: &[u8]) -> Result<TokenData<Claims>, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_rsa_pem(public_key_pem)?;
    decode::<Claims>(token, &decoding_key, &Validation::new(Algorithm::RS256))
}
```

In `src/handlers/well_known.rs`:

```rust
use actix_web::{web, HttpResponse};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::RsaPublicKey;
use rsa::traits::PublicKeyParts;
use serde_json::json;

pub async fn jwks(public_key: web::Data<RsaPublicKey>) -> HttpResponse {
    let n = URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be());
    let e = URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be());

    let jwks = json!({
        "keys": [{
            "kty": "RSA",
            "alg": "RS256",
            "use": "sig",
            "kid": "default",
            "n": n,
            "e": e
        }]
    });

    HttpResponse::Ok()
        .content_type("application/json")
        .json(jwks)
}
```

**Step 4: Wire JWKS route into main.rs**

```rust
// In main.rs, add:
.route("/.well-known/jwks.json", web::get().to(handlers::well_known::jwks))
// Pass public key as app data:
.app_data(web::Data::new(key_pair.public_key.clone()))
```

**Step 5: Verify compilation**

Run: `cargo build`
Expected: Successful compilation

**Step 6: Commit**

```bash
git add src/auth/ src/handlers/ certs/README.md Cargo.lock
git commit -m "feat: add RSA key management and JWKS endpoint"
```

---

### Task CRUISE-003: JWT Auth Middleware

**Files:**
- Create: `src/auth/middleware.rs`
- Modify: `src/auth/mod.rs`

**Step 1: Write failing test for middleware extraction**

In `src/auth/middleware.rs`:

```rust
use actix_web::{dev::ServiceRequest, Error, HttpMessage};
use actix_web::error::ErrorUnauthorized;

use super::jwt::{validate_token, Claims};

/// Extract and validate JWT from Authorization header.
/// Sets Claims in request extensions if valid.
pub fn extract_claims(req: &ServiceRequest, public_key_pem: &[u8]) -> Result<Claims, Error> {
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ErrorUnauthorized("Missing Authorization header"))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ErrorUnauthorized("Invalid Authorization format"))?;

    let token_data = validate_token(token, public_key_pem)
        .map_err(|e| ErrorUnauthorized(format!("Invalid token: {}", e)))?;

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    // Test that missing header returns error
    // Test that valid token returns claims
    // (Requires creating a test token with known key)
}
```

**Step 2: Create actix middleware wrapper**

```rust
use actix_web::middleware::from_fn;
use actix_web::{web, body::MessageBody, dev::{ServiceRequest, ServiceResponse}, middleware::Next};

pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let public_key_pem = req.app_data::<web::Data<Vec<u8>>>()
        .expect("public key PEM in app data");

    let claims = extract_claims(&req, public_key_pem)?;
    req.extensions_mut().insert(claims);

    next.call(req).await
}
```

**Step 3: Run tests**

Run: `cargo test --lib auth`
Expected: PASS

**Step 4: Commit**

```bash
git add src/auth/middleware.rs src/auth/mod.rs
git commit -m "feat: add JWT auth middleware"
```

---

### Task CRUISE-004: SQLite Database Layer

**Files:**
- Create: `src/db/mod.rs`
- Create: `src/db/connection.rs`
- Create: `src/db/schema.rs`
- Create: `src/db/operations.rs`

**Step 1: Write tests for schema introspection**

In `src/db/schema.rs`:

```rust
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TableInfo {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub cid: i32,
    pub name: String,
    pub col_type: String,
    pub notnull: bool,
    pub dflt_value: Option<String>,
    pub pk: bool,
}

pub fn list_tables(conn: &Connection) -> rusqlite::Result<Vec<TableInfo>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )?;
    let tables = stmt.query_map([], |row| {
        Ok(TableInfo { name: row.get(0)? })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(tables)
}

pub fn list_columns(conn: &Connection, table_name: &str) -> rusqlite::Result<Vec<ColumnInfo>> {
    // Validate table name to prevent SQL injection (alphanumeric + underscore only)
    if !table_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(rusqlite::Error::InvalidParameterName(format!("Invalid table name: {}", table_name)));
    }
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table_name))?;
    let columns = stmt.query_map([], |row| {
        Ok(ColumnInfo {
            cid: row.get(0)?,
            name: row.get(1)?,
            col_type: row.get(2)?,
            notnull: row.get(3)?,
            dflt_value: row.get(4)?,
            pk: row.get::<_, i32>(5)? != 0,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_tables_empty() {
        let conn = Connection::open_in_memory().unwrap();
        let tables = list_tables(&conn).unwrap();
        assert!(tables.is_empty());
    }

    #[test]
    fn test_list_tables_after_create() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY)", []).unwrap();
        let tables = list_tables(&conn).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "users");
    }

    #[test]
    fn test_list_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)", []).unwrap();
        let cols = list_columns(&conn, "users").unwrap();
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].name, "id");
        assert_eq!(cols[1].name, "name");
        assert!(cols[1].notnull);
    }
}
```

**Step 2: Run tests**

Run: `cargo test --lib db::schema::tests`
Expected: PASS

**Step 3: Write DDL operations with tests**

In `src/db/operations.rs`:

```rust
use rusqlite::Connection;

/// Validate identifier (table/column names) - alphanumeric and underscore only.
fn validate_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(format!("Invalid identifier: {}", name));
    }
    if name.starts_with("sqlite_") {
        return Err("Cannot use sqlite_ prefix".into());
    }
    Ok(())
}

pub fn create_table(conn: &Connection, name: &str) -> Result<(), String> {
    validate_identifier(name)?;
    conn.execute(
        &format!("CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT)", name),
        [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn drop_table(conn: &Connection, name: &str) -> Result<(), String> {
    validate_identifier(name)?;
    conn.execute(&format!("DROP TABLE \"{}\"", name), [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn add_column(conn: &Connection, table: &str, column: &str, col_type: &str) -> Result<(), String> {
    validate_identifier(table)?;
    validate_identifier(column)?;
    // Allow only safe type names
    let allowed_types = ["TEXT", "INTEGER", "REAL", "BLOB", "NUMERIC"];
    let col_type_upper = col_type.to_uppercase();
    if !allowed_types.contains(&col_type_upper.as_str()) {
        return Err(format!("Invalid column type: {}. Allowed: {:?}", col_type, allowed_types));
    }
    conn.execute(
        &format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}", table, column, col_type_upper),
        [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn drop_column(conn: &Connection, table: &str, column: &str) -> Result<(), String> {
    validate_identifier(table)?;
    validate_identifier(column)?;
    // SQLite >= 3.35.0 supports DROP COLUMN
    conn.execute(
        &format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\"", table, column),
        [],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::{list_tables, list_columns};

    #[test]
    fn test_create_and_drop_table() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn, "test_table").unwrap();
        let tables = list_tables(&conn).unwrap();
        assert_eq!(tables.len(), 1);

        drop_table(&conn, "test_table").unwrap();
        let tables = list_tables(&conn).unwrap();
        assert!(tables.is_empty());
    }

    #[test]
    fn test_add_and_drop_column() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn, "test_table").unwrap();
        add_column(&conn, "test_table", "name", "TEXT").unwrap();

        let cols = list_columns(&conn, "test_table").unwrap();
        assert_eq!(cols.len(), 2); // id + name

        drop_column(&conn, "test_table", "name").unwrap();
        let cols = list_columns(&conn, "test_table").unwrap();
        assert_eq!(cols.len(), 1); // just id
    }

    #[test]
    fn test_sql_injection_prevention() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(create_table(&conn, "test; DROP TABLE users").is_err());
        assert!(create_table(&conn, "").is_err());
        assert!(create_table(&conn, "sqlite_reserved").is_err());
    }

    #[test]
    fn test_invalid_column_type() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn, "test_table").unwrap();
        assert!(add_column(&conn, "test_table", "col", "EVIL_TYPE").is_err());
    }
}
```

**Step 4: Run all DB tests**

Run: `cargo test --lib db`
Expected: PASS

**Step 5: Write connection pool setup**

In `src/db/connection.rs`:

```rust
use rusqlite::Connection;
use std::sync::Mutex;

pub struct DbPool {
    conn: Mutex<Connection>,
}

impl DbPool {
    pub fn new(path: &str) -> Self {
        let conn = Connection::open(path).expect("open SQLite database");
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .expect("set SQLite pragmas");
        DbPool { conn: Mutex::new(conn) }
    }

    pub fn get(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("lock database connection")
    }
}
```

**Step 6: Commit**

```bash
git add src/db/
git commit -m "feat: add SQLite database layer with schema introspection and DDL operations"
```

---

### Task CRUISE-005: HTML Templates with htmx

**Files:**
- Create: `src/templates/base.html`
- Create: `src/templates/login.html`
- Create: `src/templates/tables.html`
- Create: `src/templates/table_row.html`
- Create: `src/templates/columns.html`
- Create: `src/templates/column_row.html`

**Step 1: Create base layout**

`src/templates/base.html`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SQLite Web Editor</title>
    <script src="https://unpkg.com/htmx.org@2.0.4"></script>
    <style>
        body { font-family: system-ui, sans-serif; max-width: 960px; margin: 0 auto; padding: 1rem; }
        table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
        th, td { border: 1px solid #ddd; padding: 0.5rem; text-align: left; }
        th { background: #f5f5f5; }
        button { padding: 0.4rem 0.8rem; cursor: pointer; }
        .btn-danger { background: #dc3545; color: white; border: none; border-radius: 3px; }
        .btn-primary { background: #0d6efd; color: white; border: none; border-radius: 3px; }
        input, select { padding: 0.4rem; margin: 0.2rem; }
        .error { color: #dc3545; padding: 0.5rem; }
        .success { color: #198754; padding: 0.5rem; }
    </style>
</head>
<body>
    <h1>SQLite Web Editor</h1>
    {% block content %}{% endblock %}
</body>
</html>
```

**Step 2: Create login template**

`src/templates/login.html`:

```html
{% extends "base.html" %}
{% block content %}
<div id="login-form">
    <h2>Login</h2>
    <form hx-post="/auth/login" hx-target="#login-form" hx-swap="outerHTML">
        <label for="token">JWT Token:</label><br>
        <textarea id="token" name="token" rows="4" cols="60" placeholder="Paste your JWT token here..." required></textarea><br>
        <button type="submit" class="btn-primary">Login</button>
    </form>
    {% if error %}
    <p class="error">{{ error }}</p>
    {% endif %}
</div>
{% endblock %}
```

**Step 3: Create tables view template**

`src/templates/tables.html`:

```html
{% extends "base.html" %}
{% block content %}
<h2>Tables</h2>
<div id="tables-container">
    <table>
        <thead>
            <tr>
                <th>Table Name</th>
                <th>Actions</th>
            </tr>
        </thead>
        <tbody id="table-list">
            {% for table in tables %}
            {% include "table_row.html" %}
            {% endfor %}
        </tbody>
    </table>

    <h3>Create New Table</h3>
    <form hx-post="/api/tables" hx-target="#table-list" hx-swap="beforeend" hx-on::after-request="this.reset()">
        <input type="text" name="name" placeholder="Table name" required pattern="[a-zA-Z_][a-zA-Z0-9_]*">
        <button type="submit" class="btn-primary">Create Table</button>
    </form>
    <div id="table-message"></div>
</div>
{% endblock %}
```

**Step 4: Create table row partial**

`src/templates/table_row.html`:

```html
<tr id="table-{{ table.name }}">
    <td><a href="/tables/{{ table.name }}/columns" hx-get="/tables/{{ table.name }}/columns" hx-target="#tables-container" hx-swap="innerHTML" hx-push-url="true">{{ table.name }}</a></td>
    <td>
        <button class="btn-danger" hx-delete="/api/tables/{{ table.name }}" hx-target="#table-{{ table.name }}" hx-swap="outerHTML" hx-confirm="Delete table '{{ table.name }}'?">Drop</button>
    </td>
</tr>
```

**Step 5: Create columns view and row partial**

`src/templates/columns.html`:

```html
<h2>Columns in: {{ table_name }}</h2>
<a href="/tables" hx-get="/tables" hx-target="#tables-container" hx-swap="innerHTML" hx-push-url="true">&larr; Back to Tables</a>

<table>
    <thead>
        <tr>
            <th>Name</th>
            <th>Type</th>
            <th>Not Null</th>
            <th>Primary Key</th>
            <th>Actions</th>
        </tr>
    </thead>
    <tbody id="column-list">
        {% for column in columns %}
        {% include "column_row.html" %}
        {% endfor %}
    </tbody>
</table>

<h3>Add Column</h3>
<form hx-post="/api/tables/{{ table_name }}/columns" hx-target="#column-list" hx-swap="beforeend" hx-on::after-request="this.reset()">
    <input type="text" name="name" placeholder="Column name" required pattern="[a-zA-Z_][a-zA-Z0-9_]*">
    <select name="col_type">
        <option value="TEXT">TEXT</option>
        <option value="INTEGER">INTEGER</option>
        <option value="REAL">REAL</option>
        <option value="BLOB">BLOB</option>
        <option value="NUMERIC">NUMERIC</option>
    </select>
    <button type="submit" class="btn-primary">Add Column</button>
</form>
```

`src/templates/column_row.html`:

```html
<tr id="col-{{ column.name }}">
    <td>{{ column.name }}</td>
    <td>{{ column.col_type }}</td>
    <td>{{ column.notnull }}</td>
    <td>{{ column.pk }}</td>
    <td>
        {% if not column.pk %}
        <button class="btn-danger" hx-delete="/api/tables/{{ table_name }}/columns/{{ column.name }}" hx-target="#col-{{ column.name }}" hx-swap="outerHTML" hx-confirm="Drop column '{{ column.name }}'?">Drop</button>
        {% endif %}
    </td>
</tr>
```

**Step 6: Commit**

```bash
git add src/templates/
git commit -m "feat: add htmx-based HTML templates"
```

---

### Task CRUISE-006: HTTP Handlers (Auth, Tables, Columns)

**Files:**
- Create: `src/handlers/auth.rs`
- Create: `src/handlers/tables.rs`
- Create: `src/handlers/columns.rs`
- Modify: `src/handlers/mod.rs`
- Modify: `src/main.rs`
- Create: `src/config.rs`

**Step 1: Write auth handler**

`src/handlers/auth.rs`:

```rust
use actix_web::{web, HttpRequest, HttpResponse};
use tera::Tera;

pub async fn login_page(tera: web::Data<Tera>) -> HttpResponse {
    let ctx = tera::Context::new();
    let body = tera.render("login.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    token: String,
}

pub async fn login_submit(
    form: web::Form<LoginForm>,
    public_key_pem: web::Data<Vec<u8>>,
    tera: web::Data<Tera>,
) -> HttpResponse {
    match crate::auth::jwt::validate_token(&form.token, &public_key_pem) {
        Ok(_claims) => {
            // Return redirect to tables page with token in cookie
            HttpResponse::SeeOther()
                .append_header(("Set-Cookie", format!("token={}; HttpOnly; Path=/; SameSite=Strict", form.token)))
                .append_header(("Location", "/tables"))
                .finish()
        }
        Err(e) => {
            let mut ctx = tera::Context::new();
            ctx.insert("error", &format!("Invalid token: {}", e));
            let body = tera.render("login.html", &ctx).unwrap();
            HttpResponse::Unauthorized().content_type("text/html").body(body)
        }
    }
}
```

**Step 2: Write table handlers**

`src/handlers/tables.rs`:

```rust
use actix_web::{web, HttpResponse};
use tera::Tera;
use crate::db::DbPool;
use crate::db::{schema, operations};

pub async fn list_tables(pool: web::Data<DbPool>, tera: web::Data<Tera>) -> HttpResponse {
    let conn = pool.get();
    let tables = schema::list_tables(&conn).unwrap_or_default();
    let mut ctx = tera::Context::new();
    ctx.insert("tables", &tables);
    let body = tera.render("tables.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[derive(serde::Deserialize)]
pub struct CreateTable {
    name: String,
}

pub async fn create_table(
    pool: web::Data<DbPool>,
    form: web::Form<CreateTable>,
    tera: web::Data<Tera>,
) -> HttpResponse {
    let conn = pool.get();
    match operations::create_table(&conn, &form.name) {
        Ok(()) => {
            let table = schema::TableInfo { name: form.name.clone() };
            let mut ctx = tera::Context::new();
            ctx.insert("table", &table);
            let body = tera.render("table_row.html", &ctx).unwrap();
            HttpResponse::Ok().content_type("text/html").body(body)
        }
        Err(e) => HttpResponse::BadRequest().body(format!("<p class=\"error\">{}</p>", e)),
    }
}

pub async fn drop_table(pool: web::Data<DbPool>, path: web::Path<String>) -> HttpResponse {
    let table_name = path.into_inner();
    let conn = pool.get();
    match operations::drop_table(&conn, &table_name) {
        Ok(()) => HttpResponse::Ok().body(""),
        Err(e) => HttpResponse::BadRequest().body(format!("<p class=\"error\">{}</p>", e)),
    }
}
```

**Step 3: Write column handlers**

`src/handlers/columns.rs`:

```rust
use actix_web::{web, HttpResponse};
use tera::Tera;
use crate::db::DbPool;
use crate::db::{schema, operations};

pub async fn list_columns(
    pool: web::Data<DbPool>,
    path: web::Path<String>,
    tera: web::Data<Tera>,
) -> HttpResponse {
    let table_name = path.into_inner();
    let conn = pool.get();
    let columns = schema::list_columns(&conn, &table_name).unwrap_or_default();
    let mut ctx = tera::Context::new();
    ctx.insert("table_name", &table_name);
    ctx.insert("columns", &columns);
    let body = tera.render("columns.html", &ctx).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[derive(serde::Deserialize)]
pub struct AddColumn {
    name: String,
    col_type: String,
}

pub async fn add_column(
    pool: web::Data<DbPool>,
    path: web::Path<String>,
    form: web::Form<AddColumn>,
    tera: web::Data<Tera>,
) -> HttpResponse {
    let table_name = path.into_inner();
    let conn = pool.get();
    match operations::add_column(&conn, &table_name, &form.name, &form.col_type) {
        Ok(()) => {
            let column = schema::ColumnInfo {
                cid: 0, name: form.name.clone(), col_type: form.col_type.clone(),
                notnull: false, dflt_value: None, pk: false,
            };
            let mut ctx = tera::Context::new();
            ctx.insert("table_name", &table_name);
            ctx.insert("column", &column);
            let body = tera.render("column_row.html", &ctx).unwrap();
            HttpResponse::Ok().content_type("text/html").body(body)
        }
        Err(e) => HttpResponse::BadRequest().body(format!("<p class=\"error\">{}</p>", e)),
    }
}

#[derive(serde::Deserialize)]
pub struct DropColumnPath {
    table: String,
    column: String,
}

pub async fn drop_column(
    pool: web::Data<DbPool>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (table_name, column_name) = path.into_inner();
    let conn = pool.get();
    match operations::drop_column(&conn, &table_name, &column_name) {
        Ok(()) => HttpResponse::Ok().body(""),
        Err(e) => HttpResponse::BadRequest().body(format!("<p class=\"error\">{}</p>", e)),
    }
}
```

**Step 4: Wire everything into main.rs**

```rust
use actix_web::{web, App, HttpServer, middleware as actix_mw};
use std::path::PathBuf;
use tera::Tera;

mod auth;
mod db;
mod handlers;
mod config;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let certs_dir = PathBuf::from("certs");
    let key_pair = auth::keys::load_or_generate_keys(&certs_dir);
    let public_key_pem = key_pair.public_key
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF).expect("encode public key PEM")
        .into_bytes();

    let db_pool = db::connection::DbPool::new("data.db");
    let tera = Tera::new("src/templates/**/*.html").expect("parse templates");

    let public_key = key_pair.public_key.clone();

    HttpServer::new(move || {
        App::new()
            .wrap(actix_mw::Logger::default())
            .app_data(web::Data::new(public_key.clone()))
            .app_data(web::Data::new(public_key_pem.clone()))
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::new(tera.clone()))
            // Public routes
            .route("/health", web::get().to(|| async { actix_web::HttpResponse::Ok().body("OK") }))
            .route("/.well-known/jwks.json", web::get().to(handlers::well_known::jwks))
            .route("/", web::get().to(handlers::auth::login_page))
            .route("/auth/login", web::post().to(handlers::auth::login_submit))
            // Protected routes (auth middleware)
            .service(
                web::scope("")
                    .wrap(actix_web::middleware::from_fn(auth::middleware::auth_middleware))
                    .route("/tables", web::get().to(handlers::tables::list_tables))
                    .route("/api/tables", web::post().to(handlers::tables::create_table))
                    .route("/api/tables/{name}", web::delete().to(handlers::tables::drop_table))
                    .route("/tables/{name}/columns", web::get().to(handlers::columns::list_columns))
                    .route("/api/tables/{table}/columns", web::post().to(handlers::columns::add_column))
                    .route("/api/tables/{table}/columns/{column}", web::delete().to(handlers::columns::drop_column))
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

**Step 5: Verify compilation**

Run: `cargo build`
Expected: Successful compilation

**Step 6: Manual smoke test**

Run: `cargo run &` then `curl http://localhost:8080/health`
Expected: "OK"

Run: `curl http://localhost:8080/.well-known/jwks.json`
Expected: Valid JWKS JSON

**Step 7: Commit**

```bash
git add src/handlers/ src/config.rs src/main.rs
git commit -m "feat: add HTTP handlers for auth, tables, and columns"
```

---

### Task CRUISE-007: Auth Middleware Cookie Support

**Files:**
- Modify: `src/auth/middleware.rs`

**Step 1: Update middleware to check cookie as well as header**

The auth middleware should extract the JWT token from either:
1. `Authorization: Bearer <token>` header (API clients)
2. `token` cookie (browser sessions after login)

```rust
pub fn extract_token(req: &ServiceRequest) -> Option<String> {
    // Try Authorization header first
    if let Some(auth) = req.headers().get("Authorization").and_then(|v| v.to_str().ok()) {
        if let Some(token) = auth.strip_prefix("Bearer ") {
            return Some(token.to_string());
        }
    }
    // Fall back to cookie
    if let Some(cookie) = req.cookie("token") {
        return Some(cookie.value().to_string());
    }
    None
}
```

**Step 2: Test cookie extraction**

Write unit tests that verify both extraction paths.

**Step 3: Commit**

```bash
git add src/auth/middleware.rs
git commit -m "feat: support JWT from both header and cookie"
```

---

### Task CRUISE-008: E2E Test Setup with Playwright

**Files:**
- Create: `tests/e2e/package.json`
- Create: `tests/e2e/tsconfig.json`
- Create: `tests/e2e/playwright.config.ts`
- Create: `tests/e2e/helpers/jwt.ts`

**Step 1: Create package.json**

```json
{
  "name": "sqlite-web-editor-e2e",
  "private": true,
  "scripts": {
    "test": "playwright test",
    "test:report": "playwright show-report"
  },
  "devDependencies": {
    "@playwright/test": "^1.49.0",
    "jsonwebtoken": "^9.0.0",
    "@types/jsonwebtoken": "^9.0.0"
  }
}
```

**Step 2: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "strict": true,
    "esModuleInterop": true,
    "outDir": "./dist",
    "rootDir": "./",
    "resolveJsonModule": true
  },
  "include": ["**/*.ts"]
}
```

**Step 3: Create playwright.config.ts**

```typescript
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './specs',
  timeout: 30000,
  retries: 0,
  use: {
    baseURL: 'http://127.0.0.1:8080',
    trace: 'on-first-retry',
  },
  reporter: [
    ['html', { outputFolder: 'playwright-report' }],
    ['json', { outputFile: 'test-results/results.json' }],
  ],
  webServer: {
    command: 'cargo run --manifest-path ../../Cargo.toml',
    url: 'http://127.0.0.1:8080/health',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
```

**Step 4: Create JWT helper for tests**

`tests/e2e/helpers/jwt.ts`:

```typescript
import * as jwt from 'jsonwebtoken';
import * as fs from 'fs';
import * as path from 'path';

const PRIVATE_KEY_PATH = path.resolve(__dirname, '../../../certs/private.pem');

export function generateTestToken(sub: string = 'test-user', expiresIn: string = '5m'): string {
  const privateKey = fs.readFileSync(PRIVATE_KEY_PATH, 'utf-8');
  return jwt.sign(
    { sub },
    privateKey,
    {
      algorithm: 'RS256',
      expiresIn,
      keyid: 'default',
    }
  );
}

export function generateExpiredToken(sub: string = 'test-user'): string {
  const privateKey = fs.readFileSync(PRIVATE_KEY_PATH, 'utf-8');
  return jwt.sign(
    { sub, iat: Math.floor(Date.now() / 1000) - 600 },
    privateKey,
    {
      algorithm: 'RS256',
      expiresIn: '1s', // already expired by the time it's used
      keyid: 'default',
    }
  );
}
```

**Step 5: Install dependencies**

Run: `cd tests/e2e && npm install && npx playwright install chromium`
Expected: Dependencies installed

**Step 6: Commit**

```bash
git add tests/e2e/package.json tests/e2e/tsconfig.json tests/e2e/playwright.config.ts tests/e2e/helpers/
git commit -m "feat: set up Playwright E2E test infrastructure"
```

---

### Task CRUISE-009: E2E Auth Tests

**Files:**
- Create: `tests/e2e/specs/auth.spec.ts`

**Step 1: Write auth E2E tests**

```typescript
import { test, expect } from '@playwright/test';
import { generateTestToken, generateExpiredToken } from '../helpers/jwt';

test.describe('Authentication', () => {
  test('shows login page on root', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('h2')).toContainText('Login');
    await expect(page.locator('textarea[name="token"]')).toBeVisible();
  });

  test('login with valid short-lived JWT token', async ({ page }) => {
    const token = generateTestToken('e2e-user', '5m');
    await page.goto('/');
    await page.fill('textarea[name="token"]', token);
    await page.click('button[type="submit"]');
    await page.waitForURL('/tables');
    await expect(page.locator('h2')).toContainText('Tables');
  });

  test('login with expired token shows error', async ({ page }) => {
    const token = generateExpiredToken('e2e-user');
    await page.goto('/');
    await page.fill('textarea[name="token"]', token);
    await page.click('button[type="submit"]');
    await expect(page.locator('.error')).toBeVisible();
  });

  test('accessing protected route without token redirects to login', async ({ page }) => {
    const response = await page.goto('/tables');
    // Should get 401 or redirect to login
    expect(response?.status()).toBeLessThanOrEqual(401);
  });

  test('.well-known/jwks.json returns valid JWKS', async ({ request }) => {
    const response = await request.get('/.well-known/jwks.json');
    expect(response.ok()).toBeTruthy();
    const body = await response.json();
    expect(body.keys).toHaveLength(1);
    expect(body.keys[0].kty).toBe('RSA');
    expect(body.keys[0].alg).toBe('RS256');
  });
});
```

**Step 2: Run tests (server must be running with generated keys)**

Run: `cd tests/e2e && npx playwright test specs/auth.spec.ts`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/e2e/specs/auth.spec.ts
git commit -m "test: add E2E authentication tests"
```

---

### Task CRUISE-010: E2E Table CRUD Tests

**Files:**
- Create: `tests/e2e/specs/tables.spec.ts`

**Step 1: Write table CRUD E2E tests**

```typescript
import { test, expect, Page } from '@playwright/test';
import { generateTestToken } from '../helpers/jwt';

async function loginAndGoToTables(page: Page) {
  const token = generateTestToken('e2e-user', '5m');
  await page.goto('/');
  await page.fill('textarea[name="token"]', token);
  await page.click('button[type="submit"]');
  await page.waitForURL('/tables');
}

test.describe('Table Management', () => {
  test.beforeEach(async ({ page }) => {
    await loginAndGoToTables(page);
  });

  test('create a new table', async ({ page }) => {
    await page.fill('input[name="name"]', 'test_users');
    await page.click('button:has-text("Create Table")');
    await expect(page.locator('#table-list')).toContainText('test_users');
  });

  test('drop a table', async ({ page }) => {
    // Create first
    await page.fill('input[name="name"]', 'to_delete');
    await page.click('button:has-text("Create Table")');
    await expect(page.locator('#table-list')).toContainText('to_delete');

    // Accept confirmation dialog
    page.on('dialog', dialog => dialog.accept());

    // Drop it
    await page.locator('#table-to_delete button.btn-danger').click();
    await expect(page.locator('#table-to_delete')).not.toBeVisible();
  });

  test('cannot create table with invalid name', async ({ page }) => {
    // HTML pattern validation will prevent submission for most cases.
    // Test server-side validation by bypassing pattern:
    await page.evaluate(() => {
      const input = document.querySelector('input[name="name"]') as HTMLInputElement;
      input.removeAttribute('pattern');
    });
    await page.fill('input[name="name"]', 'bad name!');
    await page.click('button:has-text("Create Table")');
    await expect(page.locator('.error')).toBeVisible();
  });
});
```

**Step 2: Run tests**

Run: `cd tests/e2e && npx playwright test specs/tables.spec.ts`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/e2e/specs/tables.spec.ts
git commit -m "test: add E2E table CRUD tests"
```

---

### Task CRUISE-011: E2E Column Management Tests

**Files:**
- Create: `tests/e2e/specs/columns.spec.ts`

**Step 1: Write column management E2E tests**

```typescript
import { test, expect, Page } from '@playwright/test';
import { generateTestToken } from '../helpers/jwt';

async function loginAndCreateTable(page: Page, tableName: string) {
  const token = generateTestToken('e2e-user', '5m');
  await page.goto('/');
  await page.fill('textarea[name="token"]', token);
  await page.click('button[type="submit"]');
  await page.waitForURL('/tables');

  await page.fill('input[name="name"]', tableName);
  await page.click('button:has-text("Create Table")');
  await expect(page.locator('#table-list')).toContainText(tableName);
}

test.describe('Column Management', () => {
  const TABLE_NAME = 'col_test_table';

  test.beforeEach(async ({ page }) => {
    await loginAndCreateTable(page, TABLE_NAME);
    // Navigate to columns view
    await page.locator(`a:has-text("${TABLE_NAME}")`).click();
    await expect(page.locator('h2')).toContainText(`Columns in: ${TABLE_NAME}`);
  });

  test('shows default id column', async ({ page }) => {
    await expect(page.locator('#column-list')).toContainText('id');
    await expect(page.locator('#column-list')).toContainText('INTEGER');
  });

  test('add a TEXT column', async ({ page }) => {
    await page.fill('input[name="name"]', 'username');
    await page.selectOption('select[name="col_type"]', 'TEXT');
    await page.click('button:has-text("Add Column")');
    await expect(page.locator('#column-list')).toContainText('username');
    await expect(page.locator('#column-list')).toContainText('TEXT');
  });

  test('add an INTEGER column', async ({ page }) => {
    await page.fill('input[name="name"]', 'age');
    await page.selectOption('select[name="col_type"]', 'INTEGER');
    await page.click('button:has-text("Add Column")');
    await expect(page.locator('#column-list')).toContainText('age');
  });

  test('drop a column', async ({ page }) => {
    // Add a column first
    await page.fill('input[name="name"]', 'to_drop');
    await page.selectOption('select[name="col_type"]', 'TEXT');
    await page.click('button:has-text("Add Column")');
    await expect(page.locator('#column-list')).toContainText('to_drop');

    // Accept confirmation
    page.on('dialog', dialog => dialog.accept());

    // Drop it
    await page.locator('#col-to_drop button.btn-danger').click();
    await expect(page.locator('#col-to_drop')).not.toBeVisible();
  });

  test('cannot drop primary key column', async ({ page }) => {
    // The id (PK) column should not have a drop button
    const idRow = page.locator('#col-id');
    await expect(idRow.locator('button.btn-danger')).not.toBeVisible();
  });
});
```

**Step 2: Run tests**

Run: `cd tests/e2e && npx playwright test specs/columns.spec.ts`
Expected: All tests pass

**Step 3: Commit**

```bash
git add tests/e2e/specs/columns.spec.ts
git commit -m "test: add E2E column management tests"
```

---

### Task CRUISE-012: GitHub Actions - Lint Workflow

**Files:**
- Create: `.github/workflows/lint.yml`

**Step 1: Create lint workflow**

```yaml
---
name: Lint

on:
  pull_request:
    branches: [main]

permissions: {}

jobs:
  lint:
    name: Super-Linter
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: read
      statuses: write

    steps:
      - name: Checkout code
        uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: false

      - name: Super-linter
        uses: super-linter/super-linter@v8.2.1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VALIDATE_RUST_CLIPPY: true
          VALIDATE_YAML: true
          VALIDATE_GITHUB_ACTIONS: true
          VALIDATE_HTML: true
          VALIDATE_TYPESCRIPT_ES: true
          VALIDATE_JSON: true
          DEFAULT_BRANCH: main
          # Disable linters we don't need
          VALIDATE_ALL_CODEBASE: false
```

**Step 2: Commit**

```bash
git add .github/workflows/lint.yml
git commit -m "ci: add Super-Linter workflow for PRs"
```

---

### Task CRUISE-013: GitHub Actions - Dependency Review Workflow

**Files:**
- Create: `.github/workflows/dependency-review.yml`

**Step 1: Create dependency review workflow**

```yaml
---
name: Dependency Review

on:
  pull_request:
    branches: [main]

permissions:
  contents: read

jobs:
  dependency-review:
    name: Dependency Review
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v5

      - name: Dependency Review
        uses: actions/dependency-review-action@v4
```

**Step 2: Commit**

```bash
git add .github/workflows/dependency-review.yml
git commit -m "ci: add dependency review workflow for PRs"
```

---

### Task CRUISE-014: GitHub Actions - E2E Test Workflow

**Files:**
- Create: `.github/workflows/e2e-tests.yml`

**Step 1: Create E2E test workflow**

```yaml
---
name: E2E Tests

on:
  pull_request:
    branches: [main]

permissions:
  contents: write

jobs:
  e2e:
    name: Playwright E2E Tests
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v5

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust dependencies
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build Rust server
        run: cargo build --release

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'

      - name: Install E2E dependencies
        run: |
          cd tests/e2e
          npm ci
          npx playwright install chromium --with-deps

      - name: Run E2E tests
        run: |
          cd tests/e2e
          npx playwright test

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: playwright-report
          path: tests/e2e/playwright-report/
          retention-days: 30

      - name: Commit test results
        if: always()
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add tests/e2e/test-results/ || true
          git diff --cached --quiet || git commit -m "test: update E2E test results [skip ci]"
          git push || true
```

**Step 2: Commit**

```bash
git add .github/workflows/e2e-tests.yml
git commit -m "ci: add E2E test workflow with result publishing"
```

---

### Task CRUISE-015: Final Integration and Smoke Test

**Files:**
- Modify: `src/main.rs` (final wiring if needed)
- Modify: `Cargo.toml` (add `tempfile` to dev-dependencies)

**Step 1: Add dev-dependency for tests**

In `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run full unit test suite**

Run: `cargo test`
Expected: All unit tests pass

**Step 3: Run full E2E test suite**

Run: `cd tests/e2e && npx playwright test`
Expected: All E2E tests pass

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat: complete SQLite web editor with auth, UI, and CI/CD"
```

---

## Task Summary (JSON)

```json
{
  "title": "SQLite Web Editor with JWT Auth and htmx",
  "overview": "A Rust web application (actix-web) that lets authenticated users edit an SQLite database through an htmx-driven browser UI. JWT authentication uses a local RSA key pair with JWKS discovery. E2E tests with Playwright validate all user flows. GitHub Actions provide linting (Super-Linter), dependency review, and automated E2E testing on PRs.",
  "spawn_instances": [
    {
      "id": "SPAWN-001",
      "name": "Project Scaffolding and Configuration",
      "use_spawn_team": false,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-001"]
    },
    {
      "id": "SPAWN-002",
      "name": "Auth and Crypto Implementation",
      "use_spawn_team": true,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-002", "CRUISE-003", "CRUISE-007"]
    },
    {
      "id": "SPAWN-003",
      "name": "Database Layer Implementation",
      "use_spawn_team": true,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-004"]
    },
    {
      "id": "SPAWN-004",
      "name": "Frontend Templates",
      "use_spawn_team": false,
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "permissions": ["Read", "Write", "Edit"],
      "task_ids": ["CRUISE-005"]
    },
    {
      "id": "SPAWN-005",
      "name": "HTTP Handlers and Server Integration",
      "use_spawn_team": true,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-006"]
    },
    {
      "id": "SPAWN-006",
      "name": "E2E Test Infrastructure",
      "use_spawn_team": false,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-008"]
    },
    {
      "id": "SPAWN-007",
      "name": "E2E Test Specs",
      "use_spawn_team": false,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-009", "CRUISE-010", "CRUISE-011"]
    },
    {
      "id": "SPAWN-008",
      "name": "CI/CD Workflows",
      "use_spawn_team": false,
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "permissions": ["Read", "Write", "Edit"],
      "task_ids": ["CRUISE-012", "CRUISE-013", "CRUISE-014"]
    },
    {
      "id": "SPAWN-009",
      "name": "Integration and Smoke Testing",
      "use_spawn_team": true,
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "task_ids": ["CRUISE-015"]
    }
  ],
  "tasks": [
    {
      "id": "CRUISE-001",
      "subject": "Project scaffolding: .gitignore, Cargo.toml, minimal main.rs",
      "description": "Create the .gitignore with comprehensive exclusions (keys, certs, build artifacts, node_modules, IDE files, OS files, .env, logs, .fork-join). Create Cargo.toml with actix-web, rusqlite (bundled), jsonwebtoken, tera, rsa, serde, base64 dependencies. Create minimal src/main.rs with /health endpoint. Verify compilation with cargo build.",
      "blocked_by": [],
      "complexity": "low",
      "acceptance_criteria": [
        ".gitignore excludes: *.pem, *.key, *.crt, .env*, target/, node_modules/, .DS_Store, Thumbs.db, *.log, .fork-join/, IDE files",
        "Cargo.toml has all required dependencies",
        "cargo build succeeds",
        "curl http://127.0.0.1:8080/health returns OK"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "spawn_instance": "SPAWN-001"
    },
    {
      "id": "CRUISE-002",
      "subject": "RSA key generation and JWKS endpoint",
      "description": "Create src/auth/keys.rs with load_or_generate_keys() that generates 2048-bit RSA keys, saves to PEM files, and reloads on restart. Create src/auth/jwt.rs with validate_token() using RS256. Create src/handlers/well_known.rs serving /.well-known/jwks.json with the public key in JWK format (n, e base64url-encoded). Write unit tests for key generation and reloading.",
      "blocked_by": ["CRUISE-001"],
      "complexity": "high",
      "acceptance_criteria": [
        "RSA key pair generated and persisted to certs/ directory",
        "Keys reload correctly from disk on restart",
        "/.well-known/jwks.json returns valid JWKS with RSA key",
        "JWKS contains kty, alg, use, kid, n, e fields",
        "Unit tests pass for key generation and reload"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-002"
    },
    {
      "id": "CRUISE-003",
      "subject": "JWT auth middleware",
      "description": "Create src/auth/middleware.rs with actix-web from_fn middleware that extracts JWT from Authorization header, validates with RS256, and stores Claims in request extensions. Return 401 for missing/invalid tokens. Write unit tests.",
      "blocked_by": ["CRUISE-002"],
      "complexity": "medium",
      "acceptance_criteria": [
        "Middleware extracts Bearer token from Authorization header",
        "Valid tokens set Claims in request extensions",
        "Missing header returns 401",
        "Invalid/expired tokens return 401",
        "Unit tests cover all paths"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-002"
    },
    {
      "id": "CRUISE-004",
      "subject": "SQLite database layer",
      "description": "Create src/db/ module with connection pool (Mutex<Connection> with WAL mode), schema introspection (list_tables, list_columns from sqlite_master/PRAGMA), and DDL operations (create_table, drop_table, add_column, drop_column). All identifiers validated against SQL injection (alphanumeric + underscore only). Column types restricted to TEXT, INTEGER, REAL, BLOB, NUMERIC. Write comprehensive unit tests with in-memory SQLite.",
      "blocked_by": ["CRUISE-001"],
      "complexity": "high",
      "acceptance_criteria": [
        "Connection pool with WAL mode and foreign keys enabled",
        "list_tables returns all user tables (excludes sqlite_ internal tables)",
        "list_columns returns column metadata (name, type, notnull, pk)",
        "create_table creates table with auto-increment id",
        "drop_table removes a table",
        "add_column adds typed column to existing table",
        "drop_column removes non-PK column",
        "SQL injection attempts rejected for all identifiers",
        "Invalid column types rejected",
        "All unit tests pass"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-003"
    },
    {
      "id": "CRUISE-005",
      "subject": "htmx HTML templates",
      "description": "Create Tera templates in src/templates/: base.html (layout with htmx CDN script), login.html (JWT paste form with hx-post), tables.html (table list with create form, hx-post/hx-delete), table_row.html (partial for htmx swap), columns.html (column list with add form), column_row.html (partial for htmx swap). Use hx-target, hx-swap, hx-confirm for interactive UX.",
      "blocked_by": [],
      "complexity": "medium",
      "acceptance_criteria": [
        "base.html includes htmx 2.x script",
        "login.html has textarea for JWT + submit button",
        "tables.html lists tables with drop buttons and create form",
        "table_row.html is a swappable partial for htmx",
        "columns.html lists columns with type selector and add form",
        "column_row.html is a swappable partial with drop button (hidden for PK)",
        "All htmx attributes (hx-post, hx-delete, hx-target, hx-swap, hx-confirm) correctly set"
      ],
      "permissions": ["Read", "Write", "Edit"],
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "spawn_instance": "SPAWN-004"
    },
    {
      "id": "CRUISE-006",
      "subject": "HTTP handlers and server wiring",
      "description": "Create handlers in src/handlers/: auth.rs (login page GET, login POST with cookie set), tables.rs (list GET, create POST, drop DELETE), columns.rs (list GET, add POST, drop DELETE). Wire all routes in main.rs with auth middleware on protected routes. Public routes: /, /auth/login, /health, /.well-known/jwks.json. Protected routes: /tables, /api/tables/*, /tables/*/columns, /api/tables/*/columns/*.",
      "blocked_by": ["CRUISE-002", "CRUISE-003", "CRUISE-004", "CRUISE-005"],
      "complexity": "high",
      "acceptance_criteria": [
        "GET / serves login page",
        "POST /auth/login validates JWT and sets HttpOnly cookie",
        "GET /tables lists all tables (requires auth)",
        "POST /api/tables creates table and returns partial HTML",
        "DELETE /api/tables/{name} drops table and returns empty body",
        "GET /tables/{name}/columns lists columns (requires auth)",
        "POST /api/tables/{table}/columns adds column and returns partial HTML",
        "DELETE /api/tables/{table}/columns/{column} drops column",
        "All protected routes return 401 without valid token",
        "cargo build succeeds"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-005"
    },
    {
      "id": "CRUISE-007",
      "subject": "Auth middleware cookie support",
      "description": "Update auth middleware to also check for 'token' cookie in addition to Authorization header. Browser sessions use cookie after login; API clients use header. Write unit tests for both extraction paths.",
      "blocked_by": ["CRUISE-003"],
      "complexity": "low",
      "acceptance_criteria": [
        "Middleware checks Authorization header first, then cookie",
        "Valid token in cookie grants access",
        "Unit tests verify both extraction paths"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-002"
    },
    {
      "id": "CRUISE-008",
      "subject": "Playwright E2E test infrastructure",
      "description": "Create tests/e2e/ with package.json (@playwright/test, jsonwebtoken), tsconfig.json, playwright.config.ts (webServer pointing to cargo run, json+html reporters), and helpers/jwt.ts (generateTestToken and generateExpiredToken using the server's private key). Install dependencies and Chromium.",
      "blocked_by": ["CRUISE-001"],
      "complexity": "medium",
      "acceptance_criteria": [
        "package.json has correct dependencies",
        "playwright.config.ts starts Rust server automatically",
        "helpers/jwt.ts generates valid RS256 tokens using server's private key",
        "helpers/jwt.ts generates expired tokens for negative testing",
        "npm ci && npx playwright install chromium succeeds"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "spawn_instance": "SPAWN-006"
    },
    {
      "id": "CRUISE-009",
      "subject": "E2E authentication tests",
      "description": "Write tests/e2e/specs/auth.spec.ts: test login page renders, login with valid short-lived JWT succeeds and redirects to /tables, login with expired token shows error, accessing /tables without auth returns 401, /.well-known/jwks.json returns valid JWKS.",
      "blocked_by": ["CRUISE-006", "CRUISE-007", "CRUISE-008"],
      "complexity": "medium",
      "acceptance_criteria": [
        "Test login page renders correctly",
        "Test valid JWT login + redirect to /tables",
        "Test expired JWT shows error message",
        "Test unauthenticated access returns 401",
        "Test JWKS endpoint returns valid RSA key",
        "All tests pass with npx playwright test specs/auth.spec.ts"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "spawn_instance": "SPAWN-007"
    },
    {
      "id": "CRUISE-010",
      "subject": "E2E table CRUD tests",
      "description": "Write tests/e2e/specs/tables.spec.ts: test creating a new table (verify it appears in list), test dropping a table (verify it disappears), test invalid table name returns error. Each test logs in first using generateTestToken.",
      "blocked_by": ["CRUISE-006", "CRUISE-008"],
      "complexity": "medium",
      "acceptance_criteria": [
        "Test creating table shows it in the list via htmx",
        "Test dropping table removes it via htmx",
        "Test invalid table name shows error",
        "All tests pass with npx playwright test specs/tables.spec.ts"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "spawn_instance": "SPAWN-007"
    },
    {
      "id": "CRUISE-011",
      "subject": "E2E column management tests",
      "description": "Write tests/e2e/specs/columns.spec.ts: test default id column visible, test adding TEXT column, test adding INTEGER column, test dropping a column, test PK column has no drop button. Each test logs in and creates a table first.",
      "blocked_by": ["CRUISE-006", "CRUISE-008"],
      "complexity": "medium",
      "acceptance_criteria": [
        "Test default id column is visible",
        "Test adding TEXT column shows it in list",
        "Test adding INTEGER column shows it in list",
        "Test dropping column removes it from list",
        "Test PK column has no drop button",
        "All tests pass with npx playwright test specs/columns.spec.ts"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 300",
      "spawn_instance": "SPAWN-007"
    },
    {
      "id": "CRUISE-012",
      "subject": "GitHub Actions lint workflow",
      "description": "Create .github/workflows/lint.yml using super-linter/super-linter@v8.2.1. Trigger on pull_request to main. Enable VALIDATE_RUST_CLIPPY, VALIDATE_YAML, VALIDATE_GITHUB_ACTIONS, VALIDATE_HTML, VALIDATE_TYPESCRIPT_ES, VALIDATE_JSON. Set VALIDATE_ALL_CODEBASE to false (lint changed files only).",
      "blocked_by": [],
      "complexity": "low",
      "acceptance_criteria": [
        "Workflow triggers on PRs to main",
        "Uses super-linter/super-linter@v8.2.1",
        "Enables Rust clippy, YAML, GitHub Actions, HTML, TypeScript, JSON linters",
        "Lints only changed files (VALIDATE_ALL_CODEBASE=false)",
        "YAML is valid GitHub Actions syntax"
      ],
      "permissions": ["Read", "Write", "Edit"],
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "spawn_instance": "SPAWN-008"
    },
    {
      "id": "CRUISE-013",
      "subject": "GitHub Actions dependency review workflow",
      "description": "Create .github/workflows/dependency-review.yml using actions/dependency-review-action@v4. Trigger on pull_request to main.",
      "blocked_by": [],
      "complexity": "low",
      "acceptance_criteria": [
        "Workflow triggers on PRs to main",
        "Uses actions/dependency-review-action@v4",
        "Has correct permissions (contents: read)",
        "YAML is valid GitHub Actions syntax"
      ],
      "permissions": ["Read", "Write", "Edit"],
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "spawn_instance": "SPAWN-008"
    },
    {
      "id": "CRUISE-014",
      "subject": "GitHub Actions E2E test workflow",
      "description": "Create .github/workflows/e2e-tests.yml that installs Rust (dtolnay/rust-toolchain@stable), caches cargo dependencies, builds the server, sets up Node.js 20, installs Playwright + Chromium, runs E2E tests, uploads playwright-report artifact, and commits test-results/ to the repo.",
      "blocked_by": [],
      "complexity": "medium",
      "acceptance_criteria": [
        "Workflow triggers on PRs to main",
        "Installs Rust stable and caches cargo deps",
        "Builds server with cargo build --release",
        "Installs Node.js 20 and Playwright",
        "Runs npx playwright test",
        "Uploads playwright-report as artifact",
        "Commits test-results/ back to PR branch",
        "YAML is valid GitHub Actions syntax"
      ],
      "permissions": ["Read", "Write", "Edit"],
      "cli_params": "claude --model haiku --allowedTools Read,Write,Edit --timeout 180",
      "spawn_instance": "SPAWN-008"
    },
    {
      "id": "CRUISE-015",
      "subject": "Final integration and smoke test",
      "description": "Run cargo test to verify all unit tests pass. Run the full Playwright E2E suite to verify all tests pass. Fix any integration issues. Make a final commit.",
      "blocked_by": ["CRUISE-006", "CRUISE-007", "CRUISE-009", "CRUISE-010", "CRUISE-011", "CRUISE-012", "CRUISE-013", "CRUISE-014"],
      "complexity": "medium",
      "acceptance_criteria": [
        "cargo test passes all unit tests",
        "npx playwright test passes all E2E tests",
        "All CI workflow YAML files are valid",
        "No compilation errors or warnings",
        "Application starts and serves all routes correctly"
      ],
      "permissions": ["Read", "Write", "Edit", "Bash", "Glob", "Grep"],
      "cli_params": "claude --model sonnet --allowedTools Read,Write,Edit,Bash,Glob,Grep --timeout 600",
      "spawn_instance": "SPAWN-009"
    }
  ],
  "risks": [
    "SQLite DROP COLUMN requires version >= 3.35.0; rusqlite bundled feature should include this but needs verification at build time",
    "RSA key generation is slow (~1-2s for 2048-bit); first startup will have a delay. Consider caching or using pre-generated keys in CI",
    "htmx partial swaps require precise HTML fragment responses; mismatched hx-target IDs will silently fail",
    "SQLite single-writer limitation may cause 'database is locked' errors under concurrent requests; Mutex serializes writes but may need timeout handling",
    "Playwright webServer config with cargo run may be slow to start in CI; the 120s timeout should be sufficient but may need tuning",
    "Super-Linter enables many linters by default; even with selective VALIDATE_ flags, some linters may still run and produce noise",
    "JWT token in cookie without CSRF protection could be vulnerable to CSRF attacks; SameSite=Strict mitigates this but limits cross-origin usage",
    "Test isolation: E2E tests share the same SQLite database; tests must create uniquely-named tables or clean up after themselves to avoid interference",
    "The jsonwebtoken npm package in tests must use the same RS256 algorithm as the Rust server; key format mismatches will cause silent auth failures"
  ]
}
```

## Dependency Graph

```
CRUISE-001 (Scaffolding)
├── CRUISE-002 (Keys + JWKS) ──► CRUISE-003 (Auth middleware) ──► CRUISE-007 (Cookie support)
├── CRUISE-004 (DB layer)
├── CRUISE-008 (Playwright setup)
│
├── CRUISE-005 (Templates) ─────────────────────────────────────┐
│                                                                 │
└── CRUISE-002 + CRUISE-003 + CRUISE-004 + CRUISE-005 ──► CRUISE-006 (Handlers)
                                                                  │
CRUISE-006 + CRUISE-007 + CRUISE-008 ──► CRUISE-009 (Auth E2E tests)
CRUISE-006 + CRUISE-008 ──► CRUISE-010 (Table E2E tests)
CRUISE-006 + CRUISE-008 ──► CRUISE-011 (Column E2E tests)
                                                                  │
CRUISE-012 (Lint CI) ────────────────────────────────────────────┐│
CRUISE-013 (Dep Review CI) ─────────────────────────────────────┐││
CRUISE-014 (E2E CI) ───────────────────────────────────────────┐│││
                                                                ││││
ALL ──► CRUISE-015 (Final Integration)
```

## Parallel Execution Strategy

**Phase 1** (no dependencies): CRUISE-001, CRUISE-005, CRUISE-012, CRUISE-013, CRUISE-014
**Phase 2** (after CRUISE-001): CRUISE-002, CRUISE-004, CRUISE-008
**Phase 3** (after CRUISE-002): CRUISE-003
**Phase 4** (after CRUISE-003): CRUISE-007
**Phase 5** (after CRUISE-002+003+004+005): CRUISE-006
**Phase 6** (after CRUISE-006+007+008): CRUISE-009, CRUISE-010, CRUISE-011
**Phase 7** (all done): CRUISE-015
