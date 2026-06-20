use scraper::{self, Node::Document};
use owo_colors::{OwoColorize};
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct ModelVariantData {
    model_identifier: String,
    size: String, 
    context: String,
    input: String,
    url: String,
}

#[derive(Debug, Clone)]
pub struct OllamaModelData {
    name: String,
    description: String,
    capability_tags: Vec<String>,
    size_tags: Vec<String>,
    cloud_tag: bool,
    model_variants: Vec<ModelVariantData>,
    url: String,
}

pub async fn scrape_ollama(query: String) -> Result<Vec<OllamaModelData>, Box<dyn std::error::Error>> {
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

    // model page
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

    let target_url: String = format!("https://ollama.com/search?q={}", query);
    let client: Client = Client::new();
    let response = client.get(&target_url).send().await?;
    let html_content: String = response.text().await?;
    let document = scraper::Html::parse_document(&html_content);

    // search page parse selectors
    let model_cards_selector = scraper::Selector::parse("ul[role='list'] > li[x-test-model]")?;
    let model_link_selector = scraper::Selector::parse("a.group.w-full")?;
    let name_selector = scraper::Selector::parse("div.flex.flex-col.mb-1")?;
    let desc_selector = scraper::Selector::parse("p.max-w-lg.break-words.text-neutral-800.text-md")?;
    let capability_selector = scraper::Selector::parse("span[x-test-capability]")?;
    let size_tag_selector = scraper::Selector::parse("span[x-test-size]")?;
    let cloud_selector = scraper::Selector::parse("span.bg-cyan-50")?;

    let model_cards = document.select(&model_cards_selector);
    
    for model_card in model_cards {
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
        let name = name_node.text().collect::<String>().trim().to_string();

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

        println!();
        println!(
            "{} {}", 
            format!("[ollamadex]").bright_blue(), 
            format!("Name: {}, Description: {}, Capabilities: {:?}, Sizes: {:?}, Is Cloud: {}, Href: {}", name, description, capabilities, sizes, is_cloud, href).dimmed()
        );
        println!();
    }

    Ok(vec![])
}