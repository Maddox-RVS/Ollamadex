mod ollama_scraper;
mod database;

use axum::{Json, Router, extract::Query, http::StatusCode, response::IntoResponse, routing::{get, post}};
use crate::ollama_scraper::OllamaModelData;
use tokio::{net::TcpListener};
use serde_json::{Value, json};
use owo_colors::OwoColorize;
use sqlx::{Pool, Sqlite};
use axum::extract::State;
use serde::Deserialize;
use std::env;
use dotenvy;

#[derive(Deserialize)]
struct SearchParams {
    query: String,
}

enum ApiError {
    NotFound, // 404
    InvalidInput(String), // 400
    InternalError, // 500
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Not Found".to_string()),
            ApiError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error".to_string()),
        };

        let body = Json(json!({ "error": body }));
        (status, body).into_response()
    }
}

async fn query_ollama(State(app_state): State<AppState>, Query(params): Query<SearchParams>) -> Result<Json<Value>, ApiError> {
    let pool = &app_state.pool;

    let accepted_similarity_threshold: f64 = database::get_cache_similarity_threshold(&pool).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    let query: String = params.query.trim().to_lowercase();
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/search?query={}\"", "GET".green(), &query).dimmed());
    if query.is_empty() { return Err(ApiError::InvalidInput("Query cannot be empty".into())); }

    let previous_queries = database::get_non_stale_queries(&pool).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    let similarity_scored_queries: Vec<(String, f64)> = previous_queries
        .into_iter()
        .map(|q| {
            let similarity = strsim::jaro_winkler(&q, &query);
            (q, similarity)
        })
        .collect();

    let best_match: Option<(String, f64)> = similarity_scored_queries
        .into_iter()
        .max_by(|(_, score_a), (_, score_b)| score_a.partial_cmp(score_b).unwrap());

    let result: Option<String> = match best_match {
        Some((matched_query, score)) if score >= accepted_similarity_threshold => Some(matched_query),
        _ => None,
    };

    match result {
        Some(matched_query) => {
            // find models relevant to the matched query
            let query_results = database::find_models_relevant_to_query(&pool, &matched_query).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            Ok(Json(json!(query_results)))
        }
        None => {
            // add the new query to the cache
            database::add_query(&pool, &query).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            // scrape models using the new query
            let scraped_models_data = ollama_scraper::scrape_ollama(&query)
                .await
                .map_err(|e| {
                    eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Scrape error:".red(), e.to_string().dimmed());
                    ApiError::InternalError
                })?;

            // save the scraped models to the database
            database::save_models(&pool, &scraped_models_data).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            // find models relevant to the query
            let query_results = database::find_models_relevant_to_query(&pool, &query).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            Ok(Json(json!(query_results)))
        }
    }
}

#[derive(Deserialize)]
struct FindModelParams {
    href: String,
    model_name: String,
}

async fn find_model(State(app_state): State<AppState>, Json(params): Json<FindModelParams>) -> Result<Json<Value>, ApiError> {
    let FindModelParams { href, model_name} = params;
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/find{}\"", "GET".green(), &href).dimmed());

    let pool = &app_state.pool;

    let model: Option<OllamaModelData> = database::find_model_by_href(&pool, &href).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    match model {
        Some(m) => Ok(Json(json!(m))),
        None => {
            // scrape models using the new query
            let scraped_models_data = ollama_scraper::scrape_ollama(&model_name)
                .await
                .map_err(|e| {
                    eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Scrape error:".red(), e.to_string().dimmed());
                    ApiError::InternalError
                })?;

            // save the scraped models to the database
            database::save_models(&pool, &scraped_models_data).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            // find the model in the database
            let model: Option<OllamaModelData> = database::find_model_by_href(&pool, &href).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            match model {
                Some(m) => Ok(Json(json!(m))),
                None => Err(ApiError::NotFound),
            }
        }
    }
}

async fn get_all_models(State(app_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/all\"", "GET".green()).dimmed());

    let pool = &app_state.pool;

    let models: Vec<OllamaModelData> = database::get_all_models(&pool).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    Ok(Json(json!(models)))
}

#[derive(Deserialize)]
struct SetCacheStaleParams {
    cache_stale_seconds: i64,
    api_key: String,
}

async fn set_cache_stale_seconds(State(app_state): State<AppState>, Json(params): Json<SetCacheStaleParams>) -> Result<Json<Value>, ApiError> {
    let SetCacheStaleParams { cache_stale_seconds, api_key } = params;
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/cache_stale_seconds\"", "POST".green()).dimmed());

    if api_key != app_state.api_key {
        println!("{} {} {}", "[ollamadex]".bright_blue(), "REJECTED".red(), "Invalid API key provided for setting cache stale seconds".dimmed());
        return Err(ApiError::InvalidInput("Invalid API key".into()));
    }

    let pool = &app_state.pool;

    database::set_time_for_stale_query(&pool, cache_stale_seconds).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    Ok(Json(json!({"message": "Cache stale seconds updated successfully"})))
}

#[derive(Deserialize)]
struct SetCacheSimilarityParams {
    cache_similarity_threshold: f64,
    api_key: String,
}

async fn set_cache_similarity_threshold(State(app_state): State<AppState>, Json(params): Json<SetCacheSimilarityParams>) -> Result<Json<Value>, ApiError> {
    let SetCacheSimilarityParams { cache_similarity_threshold, api_key } = params;
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/cache_similarity_threshold\"", "POST".green()).dimmed());
    
    if api_key != app_state.api_key {
        println!("{} {} {}", "[ollamadex]".bright_blue(), "REJECTED".red(), "Invalid API key provided for setting cache similarity threshold".dimmed());
        return Err(ApiError::InvalidInput("Invalid API key".into()));
    }

    if cache_similarity_threshold < 0.0 || cache_similarity_threshold > 1.0 {
        println!("{} {}", "[ollamadex]".bright_blue(), format!("Failed to set cache similarity, passed value of {} was not between 0 and 1", cache_similarity_threshold).dimmed());
        return Err(ApiError::InvalidInput(format!("Failed to set cache similarity, passed value of {} was not between 0 and 1", cache_similarity_threshold)));
    }

    let pool = &app_state.pool;

    database::set_cache_similarity_threshold(&pool, cache_similarity_threshold).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    Ok(Json(json!({"message": "Cache similarity threshold updated successfully"})))
}

async fn get_cache_similarity_threshold(State(app_state): State<AppState>) -> Result<Json<Value>, ApiError> {
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/cache_similarity_threshold\"", "GET".green()).dimmed());

    let pool = &app_state.pool;

    let threshold: f64 = database::get_cache_similarity_threshold(&pool).await.map_err(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
        ApiError::InternalError
    })?;

    Ok(Json(json!({"cache_similarity_threshold": threshold})))
}

#[derive(Clone)]
struct AppState {
    pool: Pool<Sqlite>,
    api_key: String,
}

fn create_app(pool: Pool<Sqlite>, api_key: String) -> Router {
    let app_state = AppState { pool, api_key };

    Router::new()
        .route("/search", get(query_ollama))
        .route("/find", post(find_model))
        .route("/all", get(get_all_models))
        .route("/cache_stale_seconds", post(set_cache_stale_seconds))
        .route("/cache_similarity_threshold", get(get_cache_similarity_threshold))
        .route("/cache_similarity_threshold", post(set_cache_similarity_threshold))
        .with_state(app_state)
}

#[tokio::main]
async fn main() {
    println!();
    println!("{}", " ██████╗ ██╗     ██╗      █████╗ ███╗   ███╗ █████╗ ██████╗ ███████╗██╗  ██╗".bright_blue());
    println!("{}", "██╔═══██╗██║     ██║     ██╔══██╗████╗ ████║██╔══██╗██╔══██╗██╔════╝╚██╗██╔╝".bright_blue());
    println!("{}", "██║   ██║██║     ██║     ███████║██╔████╔██║███████║██║  ██║█████╗   ╚███╔╝ ".bright_blue());
    println!("{}", "██║   ██║██║     ██║     ██╔══██║██║╚██╔╝██║██╔══██║██║  ██║██╔══╝   ██╔██╗ ".bright_blue());
    println!("{}", "╚██████╔╝███████╗███████╗██║  ██║██║ ╚═╝ ██║██║  ██║██████╔╝███████╗██╔╝ ██╗".bright_blue());
    println!("{}", " ╚═════╝ ╚══════╝╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═════╝ ╚══════╝╚═╝  ╚═╝".bright_blue()); 
    println!(); 

    dotenvy::dotenv().ok();
    
    let api_key = env::var("ADMIN_API_KEY").unwrap_or_else(|_| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to get ADMIN_API_KEY from environment variables:".red(), "Please set the ADMIN_API_KEY environment variable in your \".env\" file".dimmed());
        std::process::exit(1);
    });

    let port_string = env::var("PORT").unwrap_or_else(|_| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to get PORT from environment variables:".red(), "Please set the PORT environment variable in your \".env\" file".dimmed());
        std::process::exit(1);
    });

    let port: u16 = match port_string.parse::<u16>() {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "{} {} {}", 
                "[ollamadex]".bright_blue(), 
                format!("Invalid PORT value of \"{}\": ", port_string).red(), 
                "Please set the PORT environment variable to a valid number between 0 and 65535 in your \".env\" file".dimmed()
            );
            std::process::exit(1);
        }
    };

    let server_mode = env::var("SERVER_MODE").unwrap_or_else(|_| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to get SERVER_MODE from environment variables:".red(), "Please set the SERVER_MODE environment variable in your \".env\" file".dimmed());
        std::process::exit(1);
    });

    let bind_address = match server_mode.as_str() {
        "public" => format!("0.0.0.0:{}", port),
        "private" => format!("127.0.0.1:{}", port),
        _ => {
            eprintln!("{} {} {}", "[ollamadex]".bright_blue(), format!("Invalid SERVER_MODE value of \"{}\":", server_mode).red(), "Please set the SERVER_MODE environment variable to either 'public' or 'private' in your \".env\" file".dimmed());
            std::process::exit(1);
        }
    };
    
    println!("{} {} {}", "[ollamadex]".bright_blue(), "Server mode:".dimmed(), server_mode.dimmed());

    let pool = database::initialize_database().await.unwrap_or_else(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to initialize database:".red(), e.to_string().dimmed());
        std::process::exit(1);
    });

    println!("{} {}", "[ollamadex]".bright_blue(), "Initializing \"ollamadex\" server...".dimmed());

    let app = create_app(pool, api_key);

    let listener = TcpListener::bind(bind_address).await
        .unwrap_or_else(|_| {
            eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to bind to port:".red(), port.bold().red());
            std::process::exit(1);
        });

    let local_addr = listener.local_addr()
        .unwrap_or_else(|_| {
            eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to get local address:".red(), port.bold().red());
            std::process::exit(1);
        });

    println!(
        "{} {} {}", 
        "[ollamadex]".bright_blue(), 
        "Server is listening on".dimmed(),
        format!("http://{}", local_addr).green().bold()
    );

    axum::serve(listener, app).await
        .unwrap_or_else(|e| {
            eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Server failed to continue serving requests:".red(), e.dimmed());
            eprintln!("{} {}", "[ollamadex]".bright_blue(), "Shutting down server...".dimmed());
            std::process::exit(1);
        });
}   