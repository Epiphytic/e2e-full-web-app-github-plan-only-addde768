use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub cid: i32,
    pub name: String,
    pub col_type: String,
    pub notnull: bool,
    pub dflt_value: Option<String>,
    pub pk: bool,
}

/// Validate that a table/column name is safe for use in SQL
/// Only allows alphanumeric characters and underscores
fn validate_identifier(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    if name.len() > 128 {
        return Err("Name too long (max 128 characters)".to_string());
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Name can only contain alphanumeric characters and underscores".to_string());
    }
    if name.chars().next().unwrap().is_ascii_digit() {
        return Err("Name cannot start with a digit".to_string());
    }
    // Reject SQLite internal names
    let lower = name.to_lowercase();
    if lower.starts_with("sqlite_") {
        return Err("Names starting with 'sqlite_' are reserved".to_string());
    }
    Ok(())
}

pub fn list_tables(conn: &Connection) -> Result<Vec<TableInfo>, String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(|e| e.to_string())?;

    let tables = stmt
        .query_map([], |row| Ok(TableInfo { name: row.get(0)? }))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(tables)
}

pub fn create_table(
    conn: &Connection,
    name: &str,
    columns: &[(String, String)],
) -> Result<(), String> {
    validate_identifier(name)?;

    if columns.is_empty() {
        return Err("Table must have at least one column".to_string());
    }

    let mut col_defs = Vec::new();
    for (col_name, col_type) in columns {
        validate_identifier(col_name)?;
        validate_col_type(col_type)?;
        col_defs.push(format!("\"{}\" {}", col_name, col_type));
    }

    let sql = format!("CREATE TABLE \"{}\" ({})", name, col_defs.join(", "));
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn drop_table(conn: &Connection, name: &str) -> Result<(), String> {
    validate_identifier(name)?;
    let sql = format!("DROP TABLE IF EXISTS \"{}\"", name);
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_columns(conn: &Connection, table_name: &str) -> Result<Vec<ColumnInfo>, String> {
    validate_identifier(table_name)?;
    let sql = format!("PRAGMA table_info(\"{}\")", table_name);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let columns = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                cid: row.get(0)?,
                name: row.get(1)?,
                col_type: row.get(2)?,
                notnull: row.get::<_, i32>(3)? != 0,
                dflt_value: row.get(4)?,
                pk: row.get::<_, i32>(5)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(columns)
}

pub fn add_column(
    conn: &Connection,
    table_name: &str,
    col_name: &str,
    col_type: &str,
) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(col_name)?;
    validate_col_type(col_type)?;
    let sql = format!(
        "ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}",
        table_name, col_name, col_type
    );
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn drop_column(conn: &Connection, table_name: &str, col_name: &str) -> Result<(), String> {
    validate_identifier(table_name)?;
    validate_identifier(col_name)?;
    let sql = format!(
        "ALTER TABLE \"{}\" DROP COLUMN \"{}\"",
        table_name, col_name
    );
    conn.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_rows(
    conn: &Connection,
    table_name: &str,
) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), String> {
    validate_identifier(table_name)?;

    let columns = list_columns(conn, table_name)?;
    let col_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();

    let sql = format!("SELECT rowid, * FROM \"{}\" LIMIT 1000", table_name);
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let col_count = stmt.column_count();

    let rows = stmt
        .query_map([], |row| {
            let mut values = Vec::new();
            for i in 0..col_count {
                let val: rusqlite::types::Value = row.get(i)?;
                let json_val = match val {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(i) => {
                        serde_json::Value::Number(serde_json::Number::from(i))
                    }
                    rusqlite::types::Value::Real(f) => serde_json::Value::Number(
                        serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
                    ),
                    rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
                    rusqlite::types::Value::Blob(b) => {
                        serde_json::Value::String(format!("<blob {} bytes>", b.len()))
                    }
                };
                values.push(json_val);
            }
            Ok(values)
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut all_col_names = vec!["rowid".to_string()];
    all_col_names.extend(col_names);

    Ok((all_col_names, rows))
}

pub fn insert_row(
    conn: &Connection,
    table_name: &str,
    values: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    validate_identifier(table_name)?;

    if values.is_empty() {
        return Err("No values provided".to_string());
    }

    let mut col_names = Vec::new();
    let mut placeholders = Vec::new();
    let mut vals = Vec::new();

    for (col, val) in values {
        validate_identifier(col)?;
        col_names.push(format!("\"{}\"", col));
        placeholders.push("?".to_string());
        vals.push(val.clone());
    }

    let sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES ({})",
        table_name,
        col_names.join(", "),
        placeholders.join(", ")
    );

    let params: Vec<&dyn rusqlite::types::ToSql> = vals
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&sql, params.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_row(conn: &Connection, table_name: &str, rowid: i64) -> Result<(), String> {
    validate_identifier(table_name)?;
    let sql = format!("DELETE FROM \"{}\" WHERE rowid = ?1", table_name);
    let affected = conn
        .execute(&sql, params![rowid])
        .map_err(|e| e.to_string())?;
    if affected == 0 {
        return Err("Row not found".to_string());
    }
    Ok(())
}

fn validate_col_type(col_type: &str) -> Result<(), String> {
    let allowed = ["TEXT", "INTEGER", "REAL", "BLOB", "NUMERIC"];
    let upper = col_type.to_uppercase();
    if !allowed.contains(&upper.as_str()) {
        return Err(format!(
            "Invalid column type '{}'. Allowed: {:?}",
            col_type, allowed
        ));
    }
    Ok(())
}
