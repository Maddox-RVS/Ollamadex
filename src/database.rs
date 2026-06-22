use sqlx::{Pool, Sqlite, SqlitePool, sqlite::{SqliteConnectOptions}};
use crate::ollama_scraper::{OllamaModelData, ModelVariantData};
use owo_colors::{OwoColorize};
use chrono::{Duration, Utc};
use std::str::FromStr;
use serde_json;


pub async fn initialize_database() -> Result<Pool<Sqlite>, sqlx::Error> {
    println!("{} {}", "[ollamadex]".bright_blue(), "Initializing \"ollamadex\" database...".dimmed());

    let database_url: &str = "sqlite://ollamadex.db?mode=rwc";

    let options = SqliteConnectOptions::from_str(database_url)?
        .foreign_keys(true);

    let pool = SqlitePool::connect_with(options).await?;

    sqlx::query(
    "CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO app_settings (key, value) VALUES ('cache_stale_seconds', '600');" // Default cache stale time in seconds (10 minutes)
    )
    .execute(&pool)
    .await?;

    sqlx::query(
    "CREATE TABLE IF NOT EXISTS search_cache (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            query TEXT NOT NULL UNIQUE,
            searched_at TEXT NOT NULL
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
    "CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            capability_tags TEXT NOT NULL, -- Stored as JSON string
            size_tags TEXT NOT NULL,       -- Stored as JSON string
            cloud_tag BOOLEAN NOT NULL,
            url TEXT NOT NULL UNIQUE
        );"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
    "CREATE TABLE IF NOT EXISTS model_variants (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model_id INTEGER NOT NULL,          -- Links back to models(id)
            model_identifier TEXT NOT NULL,
            size TEXT NOT NULL,
            context TEXT NOT NULL,
            input TEXT NOT NULL,
            url TEXT NOT NULL,
            FOREIGN KEY(model_id) REFERENCES models(id) ON DELETE CASCADE
        );"
    )
    .execute(&pool)
    .await?;

    println!("{} {}", "[ollamadex]".bright_blue(), "Database initialized successfully".dimmed());

    Ok(pool)
}

pub async fn add_query(pool: &Pool<Sqlite>, query: &str) -> Result<(), sqlx::Error> {
    let now = Utc::now();

    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM search_cache WHERE query = ? LIMIT 1")
        .bind(query)
        .fetch_optional(pool)
        .await?;

    match existing {
        Some(_) => {
            sqlx::query("UPDATE search_cache SET searched_at = ? WHERE query = ?")
                .bind(now.to_rfc3339())
                .bind(query)
                .execute(pool)
                .await?;
        }
        None => {
            sqlx::query("INSERT INTO search_cache (query, searched_at) VALUES (?, ?)")
                .bind(query)
                .bind(now.to_rfc3339())
                .execute(pool)
                .await?;
        }
    }

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Query updated in database cache: {}", query).dimmed());

    Ok(())
}

pub async fn save_model(pool: &Pool<Sqlite>, model: &OllamaModelData) -> Result<(), sqlx::Error> {
    let capability_tags_json = serde_json::to_string(&model.capability_tags)
        .unwrap_or_else(|_| "[]".to_string());
    let size_tags_json = serde_json::to_string(&model.size_tags)
        .unwrap_or_else(|_| "[]".to_string());

    let model_id: i64 = sqlx::query_scalar(
        "INSERT INTO models (name, description, capability_tags, size_tags, cloud_tag, url)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(url) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            capability_tags = excluded.capability_tags,
            size_tags = excluded.size_tags,
            cloud_tag = excluded.cloud_tag
         RETURNING id"
    )
    .bind(&model.name)
    .bind(&model.description)
    .bind(&capability_tags_json)
    .bind(&size_tags_json)
    .bind(model.cloud_tag)
    .bind(&model.url)
    .fetch_one(pool)
    .await?;

    sqlx::query("DELETE FROM model_variants WHERE model_id = ?")
        .bind(model_id)
        .execute(pool)
        .await?;

    for variant in &model.model_variants {
        sqlx::query(
            "INSERT INTO model_variants (model_id, model_identifier, size, context, input, url)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(model_id)
        .bind(&variant.model_identifier)
        .bind(&variant.size)
        .bind(&variant.context)
        .bind(&variant.input)
        .bind(&variant.url)
        .execute(pool)
        .await?;
    }

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Model saved to database: {} - {}", model.name, model.url).dimmed());

    Ok(())
}

pub async fn set_time_for_stale_query(pool: &Pool<Sqlite>, seconds: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE app_settings SET value = ? WHERE key = 'cache_stale_seconds'")
        .bind(seconds.to_string())
        .execute(pool)
        .await?;

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Stale query time set to {} seconds", seconds).dimmed());

    Ok(())
}

pub async fn get_non_stale_queries(pool: &Pool<Sqlite>) -> Result<Vec<String>, sqlx::Error> {
    let stale_seconds_str: String = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'cache_stale_seconds'")
    .fetch_one(pool)
    .await?;

    let stale_seconds: i64 = stale_seconds_str.parse().unwrap_or(600);

    let cutoff = Utc::now() - Duration::seconds(stale_seconds);

    let queries: Vec<String> = sqlx::query_scalar("SELECT query FROM search_cache WHERE searched_at >= ?")
    .bind(cutoff.to_rfc3339())
    .fetch_all(pool)
    .await?;

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Retrieved non-stale {} queries", queries.len()).dimmed());

    Ok(queries)
}

pub async fn find_models_relevant_to_query(pool: &Pool<Sqlite>, query: &str) -> Result<Vec<OllamaModelData>, sqlx::Error> {
    let lowered_query = query.to_lowercase();

    let model_rows: Vec<(i64, String, String, String, String, bool, String)> = sqlx::query_as(
        "SELECT id, name, description, capability_tags, size_tags, cloud_tag, url
         FROM models
         WHERE LOWER(name) LIKE '%' || ? || '%'
         ORDER BY INSTR(LOWER(name), ?) ASC"
    )
    .bind(&lowered_query)
    .bind(&lowered_query)
    .fetch_all(pool)
    .await?;

    let mut results: Vec<OllamaModelData> = Vec::new();

    for (model_id, name, description, capability_tags_json, size_tags_json, cloud_tag, url) in model_rows {
        let capability_tags: Vec<String> = serde_json::from_str(&capability_tags_json)
            .unwrap_or_else(|_| Vec::new());
        let size_tags: Vec<String> = serde_json::from_str(&size_tags_json)
            .unwrap_or_else(|_| Vec::new());

        let variant_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT model_identifier, size, context, input, url
             FROM model_variants
             WHERE model_id = ?"
        )
        .bind(model_id)
        .fetch_all(pool)
        .await?;

        let model_variants: Vec<ModelVariantData> = variant_rows
            .into_iter()
            .map(|(model_identifier, size, context, input, variant_url)| ModelVariantData {
                model_identifier,
                size,
                context,
                input,
                url: variant_url,
            })
            .collect();

        results.push(OllamaModelData {
            name,
            description,
            capability_tags,
            size_tags,
            cloud_tag,
            model_variants,
            url,
        });
    }

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Found {} models relevant to query: {}", results.len(), query).dimmed());

    Ok(results)
}

pub async fn find_model_by_href(pool: &Pool<Sqlite>, href: &str) -> Result<Option<OllamaModelData>, sqlx::Error> {
    let full_url = format!("https://ollama.com{}", href);

    let model_row: Option<(i64, String, String, String, String, bool, String)> = sqlx::query_as(
        "SELECT id, name, description, capability_tags, size_tags, cloud_tag, url
         FROM models
         WHERE url = ?
         LIMIT 1"
    )
    .bind(&full_url)
    .fetch_optional(pool)
    .await?;

    let (model_id, name, description, capability_tags_json, size_tags_json, cloud_tag, url) = match model_row {
        Some(row) => row,
        None => return Ok(None),
    };

    let capability_tags: Vec<String> = serde_json::from_str(&capability_tags_json)
        .unwrap_or_else(|_| Vec::new());
    let size_tags: Vec<String> = serde_json::from_str(&size_tags_json)
        .unwrap_or_else(|_| Vec::new());

    let variant_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT model_identifier, size, context, input, url
         FROM model_variants
         WHERE model_id = ?"
    )
    .bind(model_id)
    .fetch_all(pool)
    .await?;

    let model_variants: Vec<ModelVariantData> = variant_rows
        .into_iter()
        .map(|(model_identifier, size, context, input, variant_url)| ModelVariantData {
            model_identifier,
            size,
            context,
            input,
            url: variant_url,
        })
        .collect();

    let model_data = OllamaModelData {
        name,
        description,
        capability_tags,
        size_tags,
        cloud_tag,
        model_variants,
        url,
    };

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Found model: {}", model_data.name).dimmed());

    Ok(Some(model_data))
}