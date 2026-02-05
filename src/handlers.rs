use actix_web::{web, HttpRequest, HttpResponse};
use serde::Deserialize;
use std::collections::HashMap;

use crate::auth;
use crate::db;
use crate::AppState;

// ─── JWKS Endpoint ───────────────────────────────────────────────────────────

pub async fn jwks_endpoint(state: web::Data<AppState>) -> HttpResponse {
    let jwks = auth::get_jwks(&state.jwt_public_key, &state.jwt_kid);
    HttpResponse::Ok().json(jwks)
}

// ─── Auth Endpoints ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    state: web::Data<AppState>,
    form: web::Form<LoginRequest>,
) -> HttpResponse {
    if form.username == "admin" && form.password == "admin" {
        match auth::create_token(&state.jwt_private_key, &form.username, 3600, &state.jwt_kid) {
            Ok(token) => {
                let cookie = actix_web::cookie::Cookie::build("token", &token)
                    .path("/")
                    .http_only(true)
                    .max_age(actix_web::cookie::time::Duration::hours(1))
                    .finish();

                HttpResponse::Ok()
                    .cookie(cookie)
                    .insert_header(("HX-Redirect", "/tables"))
                    .content_type("text/html")
                    .body("")
            }
            Err(e) => HttpResponse::InternalServerError().body(format!("Token creation failed: {}", e)),
        }
    } else {
        HttpResponse::Ok()
            .content_type("text/html")
            .body("<div class=\"error\">Invalid username or password</div>")
    }
}

// ─── Table API Endpoints ─────────────────────────────────────────────────────

pub async fn list_tables(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }
    let conn = state.db.lock().unwrap();
    match db::list_tables(&conn) {
        Ok(tables) => HttpResponse::Ok().json(tables),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e})),
    }
}

#[derive(Deserialize)]
pub struct CreateTableRequest {
    pub name: String,
    pub columns: Option<String>,
}

pub async fn create_table(
    req: HttpRequest,
    state: web::Data<AppState>,
    form: web::Form<CreateTableRequest>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let columns_str = form.columns.as_deref().unwrap_or("id:INTEGER");
    let columns: Vec<(String, String)> = columns_str
        .split(',')
        .filter_map(|s| {
            let parts: Vec<&str> = s.trim().splitn(2, ':').collect();
            if parts.len() == 2 {
                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            } else {
                None
            }
        })
        .collect();

    if columns.is_empty() {
        return HttpResponse::BadRequest()
            .content_type("text/html")
            .body("<div class=\"error\">Invalid column definitions. Use format: col1:TYPE,col2:TYPE</div>");
    }

    let conn = state.db.lock().unwrap();
    match db::create_table(&conn, &form.name, &columns) {
        Ok(()) => {
            match db::list_tables(&conn) {
                Ok(tables) => {
                    let html = render_table_list(&tables);
                    HttpResponse::Ok().content_type("text/html").body(html)
                }
                Err(e) => HttpResponse::InternalServerError().body(format!("Error: {}", e)),
            }
        }
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error creating table: {}</div>", e)),
    }
}

pub async fn drop_table(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let table_name = path.into_inner();
    let conn = state.db.lock().unwrap();
    match db::drop_table(&conn, &table_name) {
        Ok(()) => {
            match db::list_tables(&conn) {
                Ok(tables) => {
                    let html = render_table_list(&tables);
                    HttpResponse::Ok().content_type("text/html").body(html)
                }
                Err(e) => HttpResponse::InternalServerError().body(format!("Error: {}", e)),
            }
        }
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error dropping table: {}</div>", e)),
    }
}

// ─── Column API Endpoints ────────────────────────────────────────────────────

pub async fn list_columns(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let table_name = path.into_inner();
    let conn = state.db.lock().unwrap();
    match db::list_columns(&conn, &table_name) {
        Ok(columns) => HttpResponse::Ok().json(columns),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e})),
    }
}

#[derive(Deserialize)]
pub struct AddColumnRequest {
    pub name: String,
    pub col_type: String,
}

pub async fn add_column(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    form: web::Form<AddColumnRequest>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let table_name = path.into_inner();
    let conn = state.db.lock().unwrap();
    match db::add_column(&conn, &table_name, &form.name, &form.col_type) {
        Ok(()) => render_table_detail_fragment(&conn, &table_name),
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error adding column: {}</div>", e)),
    }
}

pub async fn drop_column(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let (table_name, col_name) = path.into_inner();
    let conn = state.db.lock().unwrap();
    match db::drop_column(&conn, &table_name, &col_name) {
        Ok(()) => render_table_detail_fragment(&conn, &table_name),
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error dropping column: {}</div>", e)),
    }
}

// ─── Row API Endpoints ───────────────────────────────────────────────────────

pub async fn list_rows(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let table_name = path.into_inner();
    let conn = state.db.lock().unwrap();
    match db::list_rows(&conn, &table_name) {
        Ok((columns, rows)) => HttpResponse::Ok().json(serde_json::json!({
            "columns": columns,
            "rows": rows,
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e})),
    }
}

pub async fn insert_row(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    form: web::Form<HashMap<String, String>>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let table_name = path.into_inner();
    let values = form.into_inner();
    let conn = state.db.lock().unwrap();
    match db::insert_row(&conn, &table_name, &values) {
        Ok(()) => render_table_detail_fragment(&conn, &table_name),
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error inserting row: {}</div>", e)),
    }
}

pub async fn delete_row(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    if let Err(e) = auth::authenticate(&req, &state) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": e}));
    }

    let (table_name, rowid_str) = path.into_inner();
    let rowid: i64 = match rowid_str.parse() {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().body("Invalid row ID"),
    };

    let conn = state.db.lock().unwrap();
    match db::delete_row(&conn, &table_name, rowid) {
        Ok(()) => render_table_detail_fragment(&conn, &table_name),
        Err(e) => HttpResponse::BadRequest()
            .content_type("text/html")
            .body(format!("<div class=\"error\">Error deleting row: {}</div>", e)),
    }
}

// ─── HTML Page Handlers ──────────────────────────────────────────────────────

pub async fn index_page(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    if auth::authenticate(&req, &state).is_ok() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/tables"))
            .finish();
    }
    HttpResponse::SeeOther()
        .insert_header(("Location", "/login"))
        .finish()
}

pub async fn login_page() -> HttpResponse {
    let html = include_str!("../templates/login.html");
    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn tables_page(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    if auth::authenticate(&req, &state).is_err() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish();
    }

    let conn = state.db.lock().unwrap();
    let tables = db::list_tables(&conn).unwrap_or_default();
    let table_list_html = render_table_list(&tables);

    let template = include_str!("../templates/tables.html");
    let html = template.replace("{{table_list}}", &table_list_html);

    HttpResponse::Ok().content_type("text/html").body(html)
}

pub async fn table_detail_page(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    if auth::authenticate(&req, &state).is_err() {
        return HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish();
    }

    let table_name = path.into_inner();
    let conn = state.db.lock().unwrap();

    let columns = match db::list_columns(&conn, &table_name) {
        Ok(cols) => cols,
        Err(e) => return HttpResponse::NotFound().body(format!("Table not found: {}", e)),
    };

    let (col_names, rows) = db::list_rows(&conn, &table_name).unwrap_or_default();

    let columns_html = render_columns_table(&table_name, &columns);
    let rows_html = render_rows_table(&table_name, &col_names, &rows, &columns);

    let template = include_str!("../templates/table_detail.html");
    let html = template
        .replace("{{table_name}}", &table_name)
        .replace("{{columns_html}}", &columns_html)
        .replace("{{rows_html}}", &rows_html);

    HttpResponse::Ok().content_type("text/html").body(html)
}

// ─── HTML Fragment Renderers ─────────────────────────────────────────────────

fn render_table_list(tables: &[db::TableInfo]) -> String {
    if tables.is_empty() {
        return "<div class=\"empty\">No tables yet. Create one above.</div>".to_string();
    }

    let mut html = String::from("<table><thead><tr><th>Table Name</th><th>Actions</th></tr></thead><tbody>");
    for table in tables {
        html.push_str(&format!(
            "<tr>\
                <td><a href=\"/tables/{name}\">{name}</a></td>\
                <td>\
                    <button class=\"btn btn-danger btn-sm\"\
                        hx-delete=\"/api/tables/{name}\"\
                        hx-target=\"#table-list\"\
                        hx-swap=\"innerHTML\"\
                        hx-confirm=\"Are you sure you want to drop table '{name}'?\">\
                        Drop\
                    </button>\
                </td>\
            </tr>",
            name = table.name
        ));
    }
    html.push_str("</tbody></table>");
    html
}

fn render_columns_table(table_name: &str, columns: &[db::ColumnInfo]) -> String {
    if columns.is_empty() {
        return "<div class=\"empty\">No columns defined.</div>".to_string();
    }

    let mut html = String::from(
        "<table><thead><tr><th>Name</th><th>Type</th><th>Not Null</th><th>Primary Key</th><th>Actions</th></tr></thead><tbody>",
    );
    for col in columns {
        let delete_btn = if !col.pk {
            format!(
                "<button class=\"btn btn-danger btn-sm\"\
                    hx-delete=\"/api/tables/{}/columns/{}\"\
                    hx-target=\"#table-detail\"\
                    hx-swap=\"innerHTML\"\
                    hx-confirm=\"Drop column '{}'?\">\
                    Drop\
                </button>",
                table_name, col.name, col.name
            )
        } else {
            String::from("<span style='color:#999'>PK</span>")
        };

        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            col.name,
            col.col_type,
            if col.notnull { "Yes" } else { "No" },
            if col.pk { "Yes" } else { "No" },
            delete_btn
        ));
    }
    html.push_str("</tbody></table>");
    html
}

fn render_rows_table(
    table_name: &str,
    col_names: &[String],
    rows: &[Vec<serde_json::Value>],
    columns: &[db::ColumnInfo],
) -> String {
    let mut form_html = format!(
        "<form hx-post=\"/api/tables/{}/rows\" hx-target=\"#table-detail\" hx-swap=\"innerHTML\"><div class=\"form-row\">",
        table_name
    );
    for col in columns {
        if !col.pk {
            form_html.push_str(&format!(
                "<input type=\"text\" name=\"{}\" placeholder=\"{} ({})\">",
                col.name, col.name, col.col_type
            ));
        }
    }
    form_html.push_str("<button type=\"submit\" class=\"btn btn-primary\">Add Row</button></div></form>");

    if rows.is_empty() {
        return format!("{}<div class=\"empty\">No data yet.</div>", form_html);
    }

    let mut html = format!("{}<table><thead><tr>", form_html);
    for col_name in col_names {
        html.push_str(&format!("<th>{}</th>", col_name));
    }
    html.push_str("<th>Actions</th></tr></thead><tbody>");

    for row in rows {
        html.push_str("<tr>");
        let rowid = row.first().and_then(|v| v.as_i64()).unwrap_or(0);
        for val in row {
            let display = match val {
                serde_json::Value::Null => "NULL".to_string(),
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            html.push_str(&format!("<td>{}</td>", display));
        }
        html.push_str(&format!(
            "<td><button class=\"btn btn-danger btn-sm\"\
                hx-delete=\"/api/tables/{}/rows/{}\"\
                hx-target=\"#table-detail\"\
                hx-swap=\"innerHTML\"\
                hx-confirm=\"Delete this row?\">\
                Delete\
            </button></td>",
            table_name, rowid
        ));
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    html
}

fn render_table_detail_fragment(
    conn: &std::sync::MutexGuard<'_, rusqlite::Connection>,
    table_name: &str,
) -> HttpResponse {
    let columns = db::list_columns(conn, table_name).unwrap_or_default();
    let (col_names, rows) = db::list_rows(conn, table_name).unwrap_or_default();

    let columns_html = render_columns_table(table_name, &columns);
    let rows_html = render_rows_table(table_name, &col_names, &rows, &columns);

    let template = include_str!("../templates/table_detail_fragment.html");
    let html = template
        .replace("{{table_name}}", table_name)
        .replace("{{columns_html}}", &columns_html)
        .replace("{{rows_html}}", &rows_html);

    HttpResponse::Ok().content_type("text/html").body(html)
}
