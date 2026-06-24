use owo_colors::{OwoColorize};
use serde::Serialize;
use reqwest::Client;
use scraper;

#[derive(Debug, Clone, Serialize)]
pub struct ModelVariantData {
    pub model_identifier: String,
    pub size: String, 
    pub context: String,
    pub input: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaModelData {
    pub name: String,
    pub description: String,
    pub capability_tags: Vec<String>,
    pub size_tags: Vec<String>,
    pub cloud_tag: bool,
    pub model_variants: Vec<ModelVariantData>,
    pub url: String,
}

pub async fn scrape_ollama(query: &String) -> Result<Vec<OllamaModelData>, Box<dyn std::error::Error>> {
    // search page
    // <ul role="list" class="grid grid-cols-1">
    // ---> <li x-test-model class="flex items-baseline border-b border-neutral-200 py-6"> ... </li>
    // ---> ---> <a href="/library/{model_name}" class="group w-full"> ... </a>
    // ---> ---> ---> <div class="flex flex-col mb-1" title="model name"> ... </div>
    // ---> ---> ---> ---> <p class="max-w-lg break-words text-neutral-800 text-md"> model description </p>
    // ---> ---> ---> <div class="flex flex-col"> ... </div>
    // ---> ---> ---> ---> <div class="flex flex-wrap space-x-2"> ... </div>
    // ---> ---> ---> ---> ---> <span x-test-capability class="inline-flex my-1 items-center rounded-md bg-indigo-50 px-2 py-[2px] text-xs font-medium text-indigo-600 sm:text-[13px]"> {compatability tag (e.g. tools, vision, thinking, etc...)} </span>
    // ---> ---> ---> ---> ---> and so on ...
    // ---> ---> ---> ---> ---> <span x-test-size class="inline-flex my-1 items-center rounded-md bg-[#ddf4ff] px-2 py-[2px] text-xs font-medium text-blue-600 sm:text-[13px]"> {model size string (e.g. 7b, 40b, 20b, 8b, etc...)} </span>
    // ---> ---> ---> ---> ---> and so on ...
    // ---> ---> ---> ---> ---> <span class="inline-flex my-1 items-center rounded-md bg-cyan-50 px-2 py-[2px] text-xs font-medium text-cyan-500 sm:text-[13px]"> cloud </span>
    // ---> and so on ...

    // model tags page
    // <div class="min-w-full divide-y divide-gray-200"> ... </div>
    // ---> <div class="group px-4 py-3"> ... </div>
    // ---> ---> <div class="grid grid-cols-12 items-center"> ... </div>
    // ---> ---> ---> <span class="flex items-center font-medium col-span-6 group text-sm"> ... </span>
    // ---> ---> ---> ---> <a href="/library/{model variant identifier}" class="group-hover:underline"> {model variant identifier} </a>
    // ---> ---> ---> (only shows if not cloud hosted) <p class="col-span-2 text-neutral-500 text-[13px]"> {model variant size} </p>
    // ---> ---> ---> (only shows if cloud hosted) <p x-test-model-tag-cost class="col-span-2 flex items-center gap-0.5 text-neutral-500 text-[13px]"> ... </p>
    // ---> ---> ---> <p class="col-span-2 text-neutral-500 text-[13px]"> {model variant context} </p>
    // ---> ---> ---> <div class="col-span-2 text-neutral-500 text-[13px]"> {model variant input} </div>
    // ---> and so on ...

    println!("{} {}", "[ollamadex]".bright_blue(), format!("Scraping Ollama search results for \"{}\"...", query).dimmed());

    let mut search_results: Vec<OllamaModelData> = vec![];

    let target_url: String = format!("https://ollama.com/search?q={}", query);
    let client: Client = Client::new();
    let response = client.get(&target_url).send().await?;
    let html_content: String = response.text().await?;

    // search page parse selectors
    let model_cards_selector = scraper::Selector::parse("ul[role='list'] > li[x-test-model]")?;
    let model_link_selector = scraper::Selector::parse("a.group.w-full")?;
    let name_selector = scraper::Selector::parse("div.flex.flex-col.mb-1")?;
    let desc_selector = scraper::Selector::parse("p.max-w-lg.break-words.text-neutral-800.text-md")?;
    let capability_selector = scraper::Selector::parse("span[x-test-capability]")?;
    let size_tag_selector = scraper::Selector::parse("span[x-test-size]")?;
    let cloud_selector = scraper::Selector::parse("span.bg-cyan-50")?;

    // model tags page parse selectors
    let table_selector = scraper::Selector::parse("div.min-w-full.divide-y.divide-gray-200")?;
    let row_selector = scraper::Selector::parse("div.group.px-4.py-3")?;
    let grid_selector = scraper::Selector::parse("div.grid.grid-cols-12.items-center")?;
    let identifier_span_selector = scraper::Selector::parse("span.flex.items-center.font-medium.col-span-6")?;
    let identifier_link_selector = scraper::Selector::parse("a.group-hover\\:underline")?;
    let cost_selector = scraper::Selector::parse("p[x-test-model-tag-cost].col-span-2")?;
    let plain_p_selector = scraper::Selector::parse("p.col-span-2.text-neutral-500")?;
    let input_selector = scraper::Selector::parse("div.col-span-2.text-neutral-500")?;

    let cards: Vec<(String, String, String, Vec<String>, Vec<String>, bool)> = {
        let document = scraper::Html::parse_document(&html_content);
        let mut cards = Vec::new();

        for model_card in document.select(&model_cards_selector) {
            let model_link = model_card
                .select(&model_link_selector)
                .next()
                .ok_or("Parse failed: model card missing its anchor tag link")?;

            let href = model_link
                .value()
                .attr("href")
                .ok_or("Parse failed: href attribute missing on anchor tag")?
                .to_string();

            let name_node = model_link
                .select(&name_selector)
                .next()
                .ok_or("Parse failed: model name container missing")?;
            let name = name_node
                .value()
                .attr("title")
                .ok_or("Parse failed: title attribute missing on name div")?
                .trim()
                .to_string();

            let desc_node = model_link
                .select(&desc_selector)
                .next()
                .ok_or("Parse failed: description paragraph missing")?;
            let description = desc_node.text().collect::<String>().trim().to_string();

            let capabilities: Vec<String> = model_link
                .select(&capability_selector)
                .map(|node| node.text().collect::<String>().trim().to_string())
                .collect();

            let sizes: Vec<String> = model_link
                .select(&size_tag_selector)
                .map(|node| node.text().collect::<String>().trim().to_string())
                .collect();

            let is_cloud = model_link.select(&cloud_selector).next().is_some();

            cards.push((href, name, description, capabilities, sizes, is_cloud));
        }

        cards
    };

    for (href, name, description, capabilities, sizes, is_cloud) in cards {
        let tags_url: String = format!("https://ollama.com{}/tags", href);
        let tags_response = client.get(&tags_url).send().await?;
        let tags_html_content: String = tags_response.text().await?;

        let model_variants: Vec<ModelVariantData> = {
            let tags_document = scraper::Html::parse_document(&tags_html_content);

            let table = tags_document
                .select(&table_selector)
                .next()
                .ok_or("Parse failed: tags table missing")?;

            let rows = table.select(&row_selector);

            let mut variants: Vec<ModelVariantData> = Vec::new();

            for row in rows {
                let grid = row
                    .select(&grid_selector)
                    .next()
                    .ok_or("Parse failed: row missing grid container")?;

                let identifier_span = grid
                    .select(&identifier_span_selector)
                    .next()
                    .ok_or("Parse failed: identifier span missing")?;

                let identifier_link = identifier_span
                    .select(&identifier_link_selector)
                    .next()
                    .ok_or("Parse failed: identifier anchor tag missing")?;

                let variant_identifier = identifier_link.text().collect::<String>().trim().to_string();

                let is_variant_cloud = grid.select(&cost_selector).next().is_some();

                let plain_paragraphs: Vec<String> = grid
                    .select(&plain_p_selector)
                    .map(|node| node.text().collect::<String>().trim().to_string())
                    .collect();

                let variant_size: String = if is_variant_cloud {
                    "Cloud Hosted".to_string()
                } else {
                    plain_paragraphs
                        .get(0)
                        .ok_or("Parse failed: size paragraph missing for non-cloud variant")?
                        .clone()
                };

                let variant_context: String = if is_variant_cloud {
                    plain_paragraphs
                        .get(0)
                        .ok_or("Parse failed: context paragraph missing for cloud variant")?
                        .clone()
                } else {
                    plain_paragraphs
                        .get(1)
                        .ok_or("Parse failed: context paragraph missing for non-cloud variant")?
                        .clone()
                };

                let input_node = grid
                    .select(&input_selector)
                    .next()
                    .ok_or("Parse failed: input div missing")?;
                let variant_input = input_node.text().collect::<String>().trim().to_string();

                variants.push(ModelVariantData {
                    model_identifier: variant_identifier,
                    size: variant_size,
                    context: variant_context,
                    input: variant_input,
                    url: tags_url.clone(),
                });
            }

            variants
        };

        let model_data = OllamaModelData {
            name,
            description,
            capability_tags: capabilities,
            size_tags: sizes,
            cloud_tag: is_cloud,
            model_variants,
            url: format!("https://ollama.com{}", href),
        };

        search_results.push(model_data);
    }

    println!("{} {} {}", "[ollamadex]".bright_blue(), "Finished scraping Ollama search results for \"{}\"".dimmed(), format!("{}", &query).dimmed());

    Ok(search_results)
}