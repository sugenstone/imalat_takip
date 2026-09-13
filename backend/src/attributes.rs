use std::collections::HashMap;

use crate::error::{AppError, AppResult};

/// MVP veri tipleri (yol haritasi 16.1).
pub const DATA_TYPES: &[&str] = &[
    "text",
    "textarea",
    "integer",
    "decimal",
    "boolean",
    "date",
    "datetime",
    "select",
    "multiselect",
    "email",
    "phone",
    "currency",
    "percentage",
];

pub fn is_valid_data_type(dt: &str) -> bool {
    DATA_TYPES.contains(&dt)
}

/// Dogrulanmis ozellik degeri - hangi kolona yazilacagini belirler.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Date(String),
    Datetime(String),
    Json(String),
}

impl TypedValue {
    /// SQL kolon adi (bind sirasinda kullanilir).
    pub fn column(&self) -> &'static str {
        match self {
            TypedValue::Text(_) => "value_text",
            TypedValue::Number(_) => "value_number",
            TypedValue::Boolean(_) => "value_boolean",
            TypedValue::Date(_) => "value_date",
            TypedValue::Datetime(_) => "value_datetime",
            TypedValue::Json(_) => "value_json",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            TypedValue::Text(s)
            | TypedValue::Date(s)
            | TypedValue::Datetime(s)
            | TypedValue::Json(s) => serde_json::Value::String(s.clone()),
            TypedValue::Number(n) => serde_json::json!(n),
            TypedValue::Boolean(b) => serde_json::json!(b),
        }
    }
}

fn as_string(v: &serde_json::Value) -> AppResult<String> {
    match v {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(AppError::BadRequest("Deger metin olmali".into())),
    }
}

/// Veri tipine gore degeri dogrula ve tipli forma cevir.
/// options: select/multiselect icin gecerli deger kumeleri.
pub fn validate_value(
    data_type: &str,
    value: &serde_json::Value,
    valid_options: Option<&Vec<String>>,
) -> AppResult<TypedValue> {
    let bad = |msg: &str| AppError::BadRequest(format!("Ozellik degeri gecersiz: {msg}"));

    match data_type {
        "text" | "textarea" | "email" | "phone" => {
            let s = as_string(value)?;
            if data_type == "email" && !s.contains('@') {
                return Err(bad("gecerli bir e-posta degil"));
            }
            Ok(TypedValue::Text(s))
        }
        "integer" => match value {
            serde_json::Value::Number(n) if n.is_i64() => Ok(TypedValue::Number(n.as_i64().unwrap() as f64)),
            serde_json::Value::String(s) => s
                .trim()
                .parse::<i64>()
                .map(|v| TypedValue::Number(v as f64))
                .map_err(|_| bad("tam sayi degil")),
            _ => Err(bad("tam sayi olmali")),
        },
        "decimal" | "currency" | "percentage" => match value {
            serde_json::Value::Number(n) => Ok(TypedValue::Number(n.as_f64().unwrap_or(f64::NAN))),
            serde_json::Value::String(s) => s
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .map(TypedValue::Number)
                .map_err(|_| bad("ondalikli sayi degil")),
            _ => Err(bad("sayi olmali")),
        },
        "boolean" => match value {
            serde_json::Value::Bool(b) => Ok(TypedValue::Boolean(*b)),
            serde_json::Value::Number(n) => Ok(TypedValue::Boolean(n.as_i64() == Some(1))),
            serde_json::Value::String(s) if s == "true" || s == "1" => Ok(TypedValue::Boolean(true)),
            serde_json::Value::String(s) if s == "false" || s == "0" => Ok(TypedValue::Boolean(false)),
            _ => Err(bad("mantiksal deger (true/false) olmali")),
        },
        "date" => {
            let s = as_string(value)?;
            // YYYY-MM-DD kabul et
            chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|_| bad("tarih formati YYYY-MM-DD olmali"))?;
            Ok(TypedValue::Date(s))
        }
        "datetime" => {
            let s = as_string(value)?;
            chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|_| bad("tarih-saat RFC3339 formatinda olmali"))?;
            Ok(TypedValue::Datetime(s))
        }
        "select" => {
            let s = as_string(value)?;
            match valid_options {
                Some(opts) if !opts.is_empty() => {
                    if opts.contains(&s) {
                        Ok(TypedValue::Text(s))
                    } else {
                        Err(bad("secenek listede yok"))
                    }
                }
                _ => Ok(TypedValue::Text(s)),
            }
        }
        "multiselect" => match value {
            serde_json::Value::Array(arr) => {
                let strs: Vec<String> = arr
                    .iter()
                    .map(as_string)
                    .collect::<AppResult<_>>()?;
                if let Some(opts) = valid_options {
                    for s in &strs {
                        if !opts.is_empty() && !opts.contains(s) {
                            return Err(bad("secenek listede yok"));
                        }
                    }
                }
                Ok(TypedValue::Json(serde_json::to_string(&strs).unwrap()))
            }
            _ => Err(bad("dizi (coklu secim) olmali")),
        },
        _ => Err(AppError::BadRequest(format!("Bilinmeyen veri tipi: {data_type}"))),
    }
}

/// Bir tanimin secenek degerlerini dondurur.
pub async fn load_options(
    db: &sqlx::SqlitePool,
    definition_id: &str,
) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT value FROM work_attribute_options WHERE attribute_definition_id = ?1 AND is_active = 1 ORDER BY sort_order",
    )
    .bind(definition_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}

/// Tanimlari (opsiyonlariyla) yukler - is kalemi formu ve detay icin.
pub async fn load_definitions_with_options(
    db: &sqlx::SqlitePool,
    work_type_id: &str,
) -> AppResult<Vec<DefinitionWithOptions>> {
    let defs: Vec<(String, String, String, String, i64, i64, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, name, key, data_type, is_required, sort_order, is_filterable, default_value
         FROM work_attribute_definitions
         WHERE work_type_id = ?1 AND archived_at IS NULL AND is_active = 1
         ORDER BY sort_order, name",
    )
    .bind(work_type_id)
    .fetch_all(db)
    .await?;

    let mut out = Vec::new();
    for (id, name, key, data_type, is_required, sort_order, is_filterable, default_value) in defs {
        let options = load_options(db, &id).await?;
        out.push(DefinitionWithOptions {
            id,
            name,
            key,
            data_type,
            is_required: is_required == 1,
            sort_order,
            is_filterable: is_filterable == 1,
            default_value,
            options,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DefinitionWithOptions {
    pub id: String,
    pub name: String,
    pub key: String,
    pub data_type: String,
    pub is_required: bool,
    pub sort_order: i64,
    pub is_filterable: bool,
    pub default_value: Option<String>,
    pub options: Vec<String>,
}

/// Tanim id -> (data_type, key) haritasi (deger yazarken tip kontrolu icin).
pub async fn definition_map(
    db: &sqlx::SqlitePool,
    work_type_id: &str,
) -> AppResult<HashMap<String, (String, String)>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, key, data_type FROM work_attribute_definitions
         WHERE work_type_id = ?1 AND archived_at IS NULL AND is_active = 1",
    )
    .bind(work_type_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(a, b, c)| (a, (b, c))).collect())
}
