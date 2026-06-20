mod ollama_scraper;

use axum::{Json, extract::Query, Router, routing::{get}, http::StatusCode, response::IntoResponse};
use serde_json::{Value, json};
use tokio::{net::TcpListener};
use owo_colors::OwoColorize;
use serde::Deserialize;
use clap::Parser;

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

async fn query_ollama(Query(params): Query<SearchParams>) -> Result<Json<Value>, ApiError> {
    let query: String = params.query;
    println!("{} {}", "[ollamadex]".bright_blue(), format!("{} \"/search?query={}\"", "GET".green(), query).dimmed());

    let query_results = ollama_scraper::scrape_ollama(query)
        .await
        .map_err(|e| {
            eprintln!("{} {} {}", "[ollamadex]".bright_blue(), "Scrape error:".red(), e.to_string().dimmed());
            ApiError::InternalError
        })?;

    Ok(Json(json!(query_results)))
}

fn create_app() -> Router {
    Router::new()
        .route("/search", get(query_ollama))
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

    println!("{} {}", "[ollamadex]".bright_blue(), "Initializing \"ollamadex\" server...".dimmed());

    let app = create_app();

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