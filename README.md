<div align="center">
  <div style="
    border: 1px solid rgba(128, 128, 128, 0.3); 
    border-radius: 8px; 
    padding: 15px; 
    display: inline-block;
    max-width: 100%;
    box-sizing: border-box;
    margin-bottom: 15px;
  ">
    <pre style="
      margin: 0; 
      background: transparent; 
      font-family: monospace;
      line-height: 1.1;
      overflow-x: auto;
    ">
 ██████╗ ██╗     ██╗      █████╗ ███╗   ███╗ █████╗ ██████╗ ███████╗██╗  ██╗
██╔═══██╗██║     ██║     ██╔══██╗████╗ ████║██╔══██╗██╔══██╗██╔════╝╚██╗██╔╝
██║   ██║██║     ██║     ███████║██╔████╔██║███████║██║  ██║█████╗   ╚███╔╝ 
██║   ██║██║     ██║     ██╔══██║██║╚██╔╝██║██╔══██║██║  ██║██╔══╝   ██╔██╗ 
╚██████╔╝███████╗███████╗██║  ██║██║ ╚═╝ ██║██║  ██║██████╔╝███████╗██╔╝ ██╗
 ╚═════╝ ╚══════╝╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═════╝ ╚══════╝╚═╝  ╚═╝</pre>
  </div>
</div>

---

Welcome to **Ollamadex**, a clean, localized index and management engine designed to tame the expanding ecosystem of local AI. Instead of jumping back and forth to a browser to find the right tool for the job, Ollamadex acts as a private directory that automatically organizes available models, their parameter sizes, capabilities, and specific variants into a single, lightning-fast database on your machine. Built to be entirely self-contained and private, it gives you a deeply queryable inventory of the local AI landscape, making it perfect for powering custom application pickers, automating model workflows, or simply keeping track of the best weights available for your hardware.

---

## Prerequisites

Before setting up Ollamadex, make sure you have the following installed:

* [Rust & Cargo](https://www.rust-lang.org/tools/install)
* [Docker](https://docs.docker.com/get-started/get-docker/)

## Installation & Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/Maddox-RVS/Ollamadex
   cd Ollamadex
   ```

## Database Schema

Ollamadex uses a structured SQLite relational database consisting of four main tables to handle application configuration, query caching, indexed models, and their respective architectural variants.

### 1. `app_settings`
> Stores persistent global key-value configuration flags.
> 
> | Column | Type | Constraints | Description |
> | :--- | :--- | :--- | :--- |
> | `key` | `TEXT` | `PRIMARY KEY` | Setting name identifier |
> | `value` | `TEXT` | `NOT NULL` | Associated configuration value (e.g., `'600'` for `cache_stale_seconds`) |

### 2. `search_cache`
> Caches scrapers' queries with timestamps to prevent unnecessary external hits.
> 
> | Column | Type | Constraints | Description |
> | :--- | :--- | :--- | :--- |
> | `id` | `INTEGER` | `PRIMARY KEY AUTOINCREMENT` | Unique cache record ID |
> | `query` | `TEXT` | `NOT NULL UNIQUE` | Scraped query string |
> | `searched_at` | `TEXT` | `NOT NULL` | ISO 8601 formatted timestamp |

### 3. `models`
> Contains metadata indexing the main, parent LLM families.
> 
> | Column | Type | Constraints | Description |
> | :--- | :--- | :--- | :--- |
> | `id` | `INTEGER` | `PRIMARY KEY AUTOINCREMENT` | Unique parent model ID |
> | `name` | `TEXT` | `NOT NULL` | General model name (e.g., `"llama3"`) |
> | `description` | `TEXT` | `NOT NULL` | Description scraped from ollama's website |
> | `capability_tags` | `TEXT` | `NOT NULL` | Capabilities serialized as a JSON array |
> | `size_tags` | `TEXT` | `NOT NULL` | Parameter size tags serialized as a JSON array |
> | `cloud_tag` | `BOOLEAN` | `NOT NULL` | Boolean flag indicating cloud availability |
> | `url` | `TEXT` | `NOT NULL UNIQUE` | Direct URL link to the model page on ollama's website |

### 4. `model_variants`
> Tracks individual weights, parameter configurations, and context sizes linked to parent models.
> 
> | Column | Type | Constraints | Description |
> | :--- | :--- | :--- | :--- |
> | `id` | `INTEGER` | `PRIMARY KEY AUTOINCREMENT` | Unique variant ID |
> | `model_id` | `INTEGER` | `NOT NULL`, `FOREIGN KEY` | References `models(id)` `ON DELETE CASCADE` |
> | `model_identifier` | `TEXT` | `NOT NULL` | Full reference string (e.g., `"llama3:8b-instruct-q8_0"`) |
> | `size` | `TEXT` | `NOT NULL` | File footprint size on disk |
> | `context` | `TEXT` | `NOT NULL` | Supported sequence context length limit |
> | `input` | `TEXT` | `NOT NULL` | Accepted input formats / modalities |
> | `url` | `TEXT` | `NOT NULL` | Direct tag manifest page reference URL |