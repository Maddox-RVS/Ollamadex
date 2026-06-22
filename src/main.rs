mod ollama_scraper;
mod database;

use axum::{Json, Router, extract::{Path, Query}, http::StatusCode, response::IntoResponse, routing::{get, post}};
use tokio::{net::TcpListener};
use serde_json::{Value, json};
use owo_colors::OwoColorize;
use sqlx::{Pool, Sqlite};
use axum::extract::State;
use serde::Deserialize;
use clap::Parser;

use crate::ollama_scraper::OllamaModelData;

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

async fn query_ollama(State(pool): State<Pool<Sqlite>>, Query(params): Query<SearchParams>) -> Result<Json<Value>, ApiError> {
    const ACCEPTED_SIMILARITY_THRESHOLD: f64 = 0.85;

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
        Some((matched_query, score)) if score >= ACCEPTED_SIMILARITY_THRESHOLD => Some(matched_query),
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
            for scraped_model in scraped_models_data {
                database::save_model(&pool, &scraped_model).await.map_err(|e| {
                    eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                    ApiError::InternalError
                })?;
            }

            // find models relevant to the query
            let query_results = database::find_models_relevant_to_query(&pool, &query).await.map_err(|e| {
                eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                ApiError::InternalError
            })?;

            Ok(Json(json!(query_results)))
        }
    }
}

async fn find_model(State(pool): State<Pool<Sqlite>>, Json(params): Json<FindModelParams>) -> Result<Json<Value>, ApiError> {
    let FindModelParams { href, model_name} = params;
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/find{}\"", "GET".green(), &href).dimmed());

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
            for scraped_model in scraped_models_data {
                database::save_model(&pool, &scraped_model).await.map_err(|e| {
                    eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Database error:".red(), e.to_string().dimmed());
                    ApiError::InternalError
                })?;
            }

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

#[derive(Deserialize)]
struct FindModelParams {
    href: String,
    model_name: String,
}

fn create_app(pool: Pool<Sqlite>) -> Router {

    Router::new()
        .route("/search", get(query_ollama))
        .route("/find", post(find_model))
        .with_state(pool)
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 3000)]
    port: u16,
}

#[tokio::main]
async fn main() {
    let args: Args = Args::parse();
    let port: u16 = args.port;

    println!();
    println!("{}", " ██████╗ ██╗     ██╗      █████╗ ███╗   ███╗ █████╗ ██████╗ ███████╗██╗  ██╗".bright_blue());
    println!("{}", "██╔═══██╗██║     ██║     ██╔══██╗████╗ ████║██╔══██╗██╔══██╗██╔════╝╚██╗██╔╝".bright_blue());
    println!("{}", "██║   ██║██║     ██║     ███████║██╔████╔██║███████║██║  ██║█████╗   ╚███╔╝ ".bright_blue());
    println!("{}", "██║   ██║██║     ██║     ██╔══██║██║╚██╔╝██║██╔══██║██║  ██║██╔══╝   ██╔██╗ ".bright_blue());
    println!("{}", "╚██████╔╝███████╗███████╗██║  ██║██║ ╚═╝ ██║██║  ██║██████╔╝███████╗██╔╝ ██╗".bright_blue());
    println!("{}", " ╚═════╝ ╚══════╝╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═════╝ ╚══════╝╚═╝  ╚═╝".bright_blue()); 
    println!();                                                                      
    
    let pool = database::initialize_database().await.unwrap_or_else(|e| {
        eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Failed to initialize database:".red(), e.to_string().dimmed());
        std::process::exit(1);
    });

    println!("{} {}", "[ollamadex]".bright_blue(), "Initializing \"ollamadex\" server...".dimmed());

    let app = create_app(pool);

    let listener = TcpListener::bind("0.0.0.0:3000").await
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