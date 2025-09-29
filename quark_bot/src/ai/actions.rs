use std::env;

use chrono::Utc;
use quark_core::helpers::dto::CoinVersion;
use teloxide::Bot;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, Message};

use crate::dependencies::BotDependencies;
use crate::message_history::handler::fetch;
use crate::pending_transactions::dto::PendingTransaction;

/// Execute trending pools fetch from GeckoTerminal
pub async fn execute_trending_pools(arguments: &serde_json::Value) -> String {
    // Parse arguments
    let network = arguments
        .get("network")
        .and_then(|v| v.as_str())
        .unwrap_or("aptos");

    let limit = arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .min(20) as u32;

    let page = arguments
        .get("page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .min(10) as u32;

    let duration = arguments
        .get("duration")
        .and_then(|v| v.as_str())
        .unwrap_or("24h");

    // Construct GeckoTerminal API URL - correct endpoint
    let mut url = format!(
        "https://api.geckoterminal.com/api/v2/networks/{}/trending_pools?page={}&duration={}",
        network, page, duration
    );

    // Add include parameter for more data
    url.push_str("&include=base_token,quote_token,dex");

    // Make HTTP request
    let client = reqwest::Client::new();
    let result = match client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let result =
                            format_trending_pools_response(&data, network, limit, duration);
                        // Ensure we never return an empty string to prevent Telegram error
                        if result.trim().is_empty() {
                            format!(
                                "📊 No trending pools found for {} network. The API returned valid data but no pools matched the criteria.",
                                network
                            )
                        } else {
                            result
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse trending pools API response: {}", e);
                        format!("❌ Error parsing API response: {}", e)
                    }
                }
            } else if response.status() == 404 {
                log::error!("Network '{}' not found in trending pools API", network);
                format!(
                    "❌ Network '{}' not found. Please check the network name and try again.",
                    network
                )
            } else if response.status() == 429 {
                log::error!("Rate limit exceeded for trending pools API");
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            } else {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                log::error!(
                    "Trending pools API request failed with status: {} - {}",
                    status,
                    error_text
                );
                format!(
                    "❌ API request failed with status: {} - {}",
                    status, error_text
                )
            }
        }
        Err(e) => {
            log::error!(
                "Network error when calling trending pools GeckoTerminal API: {}",
                e
            );
            format!("❌ Network error when calling GeckoTerminal API: {}", e)
        }
    };

    // Final safety check to prevent empty responses
    if result.trim().is_empty() {
        format!(
            "🔧 Debug: Function completed but result was empty. Network: {}, URL attempted",
            network
        )
    } else {
        result
    }
}

/// Format the trending pools API response into a readable string
fn format_trending_pools_response(
    data: &serde_json::Value,
    network: &str,
    limit: u32,
    duration: &str,
) -> String {
    let mut result = format!(
        "🔥 **Trending Pools on {} ({})**\n\n",
        network.to_uppercase(),
        duration
    );

    // Build lookup maps for tokens and DEXes from included array
    let mut token_map = std::collections::HashMap::new();
    let mut dex_map = std::collections::HashMap::new();
    if let Some(included) = data.get("included").and_then(|d| d.as_array()) {
        for item in included {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                match item.get("type").and_then(|v| v.as_str()) {
                    Some("token") => {
                        token_map.insert(id, item);
                    }
                    Some("dex") => {
                        dex_map.insert(id, item);
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(pools) = data.get("data").and_then(|d| d.as_array()) {
        let pools_to_show: Vec<_> = pools.iter().take(limit as usize).collect();
        for (index, pool) in pools_to_show.iter().enumerate() {
            if let Some(attributes) = pool.get("attributes") {
                let name = attributes
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Pool");
                let pool_address = attributes
                    .get("address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let pool_created_at = attributes
                    .get("pool_created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let fdv_usd = attributes
                    .get("fdv_usd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let market_cap_usd = attributes
                    .get("market_cap_usd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let reserve_usd = attributes
                    .get("reserve_in_usd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let base_token_price = attributes
                    .get("base_token_price_usd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let quote_token_price = attributes
                    .get("quote_token_price_usd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let price_changes = if let Some(pcp) = attributes.get("price_change_percentage") {
                    let h1 = pcp.get("h1").and_then(|v| v.as_str()).unwrap_or("0");
                    let h6 = pcp.get("h6").and_then(|v| v.as_str()).unwrap_or("0");
                    let h24 = pcp.get("h24").and_then(|v| v.as_str()).unwrap_or("0");
                    format!("1h: {}% | 6h: {}% | 24h: {}%", h1, h6, h24)
                } else {
                    "No data".to_string()
                };
                let volumes = if let Some(vol) = attributes.get("volume_usd") {
                    let h5m = vol.get("h5m").and_then(|v| v.as_str()).unwrap_or("0");
                    let h1 = vol.get("h1").and_then(|v| v.as_str()).unwrap_or("0");
                    let h6 = vol.get("h6").and_then(|v| v.as_str()).unwrap_or("0");
                    let h24 = vol.get("h24").and_then(|v| v.as_str()).unwrap_or("0");
                    format!(
                        "5m: ${} | 1h: ${} | 6h: ${} | 24h: ${}",
                        format_large_number(h5m),
                        format_large_number(h1),
                        format_large_number(h6),
                        format_large_number(h24)
                    )
                } else {
                    "No data".to_string()
                };
                let transactions = if let Some(txns) = attributes.get("transactions") {
                    let h5m = txns
                        .get("h5m")
                        .and_then(|v| v.get("buys"))
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0)
                        + txns
                            .get("h5m")
                            .and_then(|v| v.get("sells"))
                            .and_then(|s| s.as_u64())
                            .unwrap_or(0);
                    let h1 = txns
                        .get("h1")
                        .and_then(|v| v.get("buys"))
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0)
                        + txns
                            .get("h1")
                            .and_then(|v| v.get("sells"))
                            .and_then(|s| s.as_u64())
                            .unwrap_or(0);
                    let h24 = txns
                        .get("h24")
                        .and_then(|v| v.get("buys"))
                        .and_then(|b| b.as_u64())
                        .unwrap_or(0)
                        + txns
                            .get("h24")
                            .and_then(|v| v.get("sells"))
                            .and_then(|s| s.as_u64())
                            .unwrap_or(0);
                    format!("5m: {} | 1h: {} | 24h: {}", h5m, h1, h24)
                } else {
                    "No data".to_string()
                };
                let main_change_24h = attributes
                    .get("price_change_percentage")
                    .and_then(|v| v.get("h24"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("0");
                let price_change_formatted = if let Ok(change) = main_change_24h.parse::<f64>() {
                    if change >= 0.0 {
                        format!("📈 +{:.2}%", change)
                    } else {
                        format!("📉 {:.2}%", change)
                    }
                } else {
                    "➡️ 0.00%".to_string()
                };
                let liquidity_formatted = format_large_number(reserve_usd);
                let base_price_formatted = format_price(base_token_price);
                let quote_price_formatted = format_price(quote_token_price);
                let fdv_formatted = format_large_number(fdv_usd);
                let mcap_formatted = format_large_number(market_cap_usd);
                let created_date = if pool_created_at != "Unknown" {
                    pool_created_at.split('T').next().unwrap_or(pool_created_at)
                } else {
                    "Unknown"
                };

                // --- ENRICH WITH TOKEN & DEX INFO ---
                let (base_token_info, quote_token_info, dex_info) =
                    if let Some(relationships) = pool.get("relationships") {
                        let base_token_id = relationships
                            .get("base_token")
                            .and_then(|r| r.get("data"))
                            .and_then(|d| d.get("id"))
                            .and_then(|v| v.as_str());
                        let quote_token_id = relationships
                            .get("quote_token")
                            .and_then(|r| r.get("data"))
                            .and_then(|d| d.get("id"))
                            .and_then(|v| v.as_str());
                        let dex_id = relationships
                            .get("dex")
                            .and_then(|r| r.get("data"))
                            .and_then(|d| d.get("id"))
                            .and_then(|v| v.as_str());
                        (
                            base_token_id.and_then(|id| token_map.get(id)),
                            quote_token_id.and_then(|id| token_map.get(id)),
                            dex_id.and_then(|id| dex_map.get(id)),
                        )
                    } else {
                        (None, None, None)
                    };

                // Base token details
                let (base_name, base_symbol, base_addr, base_dec, base_cg) =
                    if let Some(token) = base_token_info {
                        let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                        (
                            attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("decimals")
                                .and_then(|v| v.as_u64())
                                .map(|d| d.to_string())
                                .unwrap_or("?".to_string()),
                            attr.get("coingecko_coin_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("-"),
                        )
                    } else {
                        ("?", "?", "?", "?".to_string(), "-")
                    };
                // Quote token details
                let (quote_name, quote_symbol, quote_addr, quote_dec, quote_cg) =
                    if let Some(token) = quote_token_info {
                        let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                        (
                            attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("decimals")
                                .and_then(|v| v.as_u64())
                                .map(|d| d.to_string())
                                .unwrap_or("?".to_string()),
                            attr.get("coingecko_coin_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("-"),
                        )
                    } else {
                        ("?", "?", "?", "?".to_string(), "-")
                    };
                // DEX details
                let dex_name = if let Some(dex) = dex_info {
                    dex.get("attributes")
                        .and_then(|a| a.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown DEX")
                } else {
                    attributes
                        .get("dex_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown DEX")
                };

                result.push_str(&format!(
                    "**{}. {} ({})** {}\n\
🔹 **Base Token:** {} ({})\n  - Address: `{}`\n  - Decimals: {}\n  - CoinGecko: {}\n\
🔹 **Quote Token:** {} ({})\n  - Address: `{}`\n  - Decimals: {}\n  - CoinGecko: {}\n\
🏦 **DEX:** {}\n\
💰 **Base Price:** ${} | **Quote Price:** ${}\n\
📊 **Volume:** {}\n\
📈 **Price Changes:** {}\n\
🔄 **Transactions:** {}\n\
💧 **Liquidity:** ${}\n\
💎 **Market Cap:** ${} | **FDV:** ${}\n\
📅 **Created:** {}\n\
🏊 **Pool:** `{}`\n\
🔗 [View on GeckoTerminal](https://www.geckoterminal.com/{}/pools/{})\n\n",
                    index + 1,
                    name,
                    dex_name,
                    price_change_formatted,
                    base_name,
                    base_symbol,
                    base_addr,
                    base_dec,
                    base_cg,
                    quote_name,
                    quote_symbol,
                    quote_addr,
                    quote_dec,
                    quote_cg,
                    dex_name,
                    base_price_formatted,
                    quote_price_formatted,
                    volumes,
                    price_changes,
                    transactions,
                    liquidity_formatted,
                    mcap_formatted,
                    fdv_formatted,
                    created_date,
                    pool_address,
                    network,
                    pool_address
                ));
            }
        }
        if pools.is_empty() {
            result.push_str("No trending pools found for this network.\n");
        } else {
            result.push_str(&format!(
                "📈 Data from GeckoTerminal • Updates every 30 seconds\n"
            ));
            result.push_str(&format!(
                "🌐 Network: {} • Showing {}/{} pools",
                network.to_uppercase(),
                pools_to_show.len(),
                pools.len()
            ));
        }
    } else {
        result.push_str("❌ No pool data found in API response.");
    }
    result
}

/// Format large numbers with appropriate suffixes (K, M, B)
fn format_large_number(num_str: &str) -> String {
    if let Ok(num) = num_str.parse::<f64>() {
        if num >= 1_000_000_000.0 {
            format!("{:.2}B", num / 1_000_000_000.0)
        } else if num >= 1_000_000.0 {
            format!("{:.2}M", num / 1_000_000.0)
        } else if num >= 1_000.0 {
            format!("{:.2}K", num / 1_000.0)
        } else {
            format!("{:.2}", num)
        }
    } else {
        "0.00".to_string()
    }
}

/// Format price with appropriate decimal places
fn format_price(price_str: &str) -> String {
    if let Ok(price) = price_str.parse::<f64>() {
        if price >= 1.0 {
            format!("{:.4}", price)
        } else if price >= 0.01 {
            format!("{:.6}", price)
        } else {
            format!("{:.8}", price)
        }
    } else {
        "0.00".to_string()
    }
}

/// Get all custom tools as a vector

/// Execute search pools fetch from GeckoTerminal
pub async fn execute_search_pools(arguments: &serde_json::Value) -> String {
    // Parse arguments
    let query = match arguments.get("query").and_then(|v| v.as_str()) {
        Some(q) if !q.trim().is_empty() => q,
        _ => {
            log::error!("Pool search called without required query parameter");
            return "❌ Error: 'query' is required for pool search.".to_string();
        }
    };
    let network = arguments.get("network").and_then(|v| v.as_str());
    let page = arguments
        .get("page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1);

    // Construct GeckoTerminal API URL
    let mut url = format!(
        "https://api.geckoterminal.com/api/v2/search/pools?query={}&page={}",
        urlencoding::encode(query),
        page
    );

    if let Some(net) = network {
        url.push_str(&format!("&network={}", urlencoding::encode(net)));
    }
    url.push_str("&include=base_token,quote_token,dex");

    // Make HTTP request
    let client = reqwest::Client::new();
    let result = match client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let result = format_search_pools_response(&data, query, network);
                        if result.trim().is_empty() {
                            format!(
                                "🔍 No pools found for query '{}'. The API returned valid data but no pools matched the criteria.",
                                query
                            )
                        } else {
                            result
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse search pools API response: {}", e);
                        format!("❌ Error parsing API response: {}", e)
                    }
                }
            } else if response.status() == 404 {
                log::error!("No pools found for query '{}' (404 response)", query);
                format!("❌ No pools found for query '{}'.", query)
            } else if response.status() == 429 {
                log::error!("Rate limit exceeded for search pools API");
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            } else {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                log::error!(
                    "Search pools API request failed with status: {} - {}",
                    status,
                    error_text
                );
                format!(
                    "❌ API request failed with status: {} - {}",
                    status, error_text
                )
            }
        }
        Err(e) => {
            log::error!(
                "Network error when calling search pools GeckoTerminal API: {}",
                e
            );
            format!("❌ Network error when calling GeckoTerminal API: {}", e)
        }
    };
    if result.trim().is_empty() {
        format!(
            "🔧 Debug: Function completed but result was empty. Query: {}",
            query
        )
    } else {
        result
    }
}

/// Format the search pools API response into a readable string
fn format_search_pools_response(
    data: &serde_json::Value,
    query: &str,
    network: Option<&str>,
) -> String {
    let mut result = String::new();
    result.push_str(&format!(
        "🔍 **Search Results for '{}'{}**\n\n",
        query,
        network.map(|n| format!(" on {}", n)).unwrap_or_default()
    ));
    // Build lookup maps for tokens and DEXes from included array
    let mut token_map = std::collections::HashMap::new();
    let mut dex_map = std::collections::HashMap::new();
    if let Some(included) = data.get("included").and_then(|d| d.as_array()) {
        for item in included {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                match item.get("type").and_then(|v| v.as_str()) {
                    Some("token") => {
                        token_map.insert(id, item);
                    }
                    Some("dex") => {
                        dex_map.insert(id, item);
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(pools) = data.get("data").and_then(|d| d.as_array()) {
        if pools.is_empty() {
            result.push_str("No pools found for this query.\n");
        } else {
            for (index, pool) in pools.iter().enumerate() {
                if let Some(attributes) = pool.get("attributes") {
                    let name = attributes
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown Pool");
                    let pool_address = attributes
                        .get("address")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let pool_created_at = attributes
                        .get("pool_created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");
                    let reserve_usd = attributes
                        .get("reserve_in_usd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    let base_token_price = attributes
                        .get("base_token_price_usd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    let quote_token_price = attributes
                        .get("quote_token_price_usd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    // --- ENRICH WITH TOKEN & DEX INFO ---
                    let (base_token_info, quote_token_info, dex_info) =
                        if let Some(relationships) = pool.get("relationships") {
                            let base_token_id = relationships
                                .get("base_token")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            let quote_token_id = relationships
                                .get("quote_token")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            let dex_id = relationships
                                .get("dex")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            (
                                base_token_id.and_then(|id| token_map.get(id)),
                                quote_token_id.and_then(|id| token_map.get(id)),
                                dex_id.and_then(|id| dex_map.get(id)),
                            )
                        } else {
                            (None, None, None)
                        };
                    // Base token details
                    let (base_name, base_symbol, base_addr) = if let Some(token) = base_token_info {
                        let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                        (
                            attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                        )
                    } else {
                        ("?", "?", "?")
                    };
                    // Quote token details
                    let (quote_name, quote_symbol, quote_addr) =
                        if let Some(token) = quote_token_info {
                            let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                            (
                                attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                                attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                                attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                            )
                        } else {
                            ("?", "?", "?")
                        };
                    // DEX details
                    let dex_name = if let Some(dex) = dex_info {
                        dex.get("attributes")
                            .and_then(|a| a.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown DEX")
                    } else {
                        attributes
                            .get("dex_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown DEX")
                    };
                    let created_date = if pool_created_at != "Unknown" {
                        pool_created_at.split('T').next().unwrap_or(pool_created_at)
                    } else {
                        "Unknown"
                    };
                    let liquidity_formatted = format_large_number(reserve_usd);
                    let base_price_formatted = format_price(base_token_price);
                    let quote_price_formatted = format_price(quote_token_price);
                    result.push_str(&format!(
                        "**{}. {} ({})**\n\
🔹 **Base Token:** {} ({})\n  - Address: `{}`\n🔹 **Quote Token:** {} ({})\n  - Address: `{}`\n💧 **Liquidity:** ${}\n💰 **Base Price:** ${} | **Quote Price:** ${}\n📅 **Created:** {}\n🏊 **Pool:** `{}`\n\
🔗 [View on GeckoTerminal](https://www.geckoterminal.com/{}/pools/{})\n\n",
                        index + 1,
                        name,
                        dex_name,
                        base_name, base_symbol, base_addr,
                        quote_name, quote_symbol, quote_addr,
                        liquidity_formatted,
                        base_price_formatted, quote_price_formatted,
                        created_date,
                        pool_address,
                        network.unwrap_or("?"),
                        pool_address
                    ));
                }
            }
            result.push_str(&format!(
                "🌐 Network: {} • Showing {}/{} pools",
                network.map(|n| n.to_uppercase()).unwrap_or_default(),
                pools.len(),
                pools.len()
            ));
        }
    } else {
        result.push_str("❌ No pool data found in API response.");
    }
    result
}

/// Execute new pools fetch from GeckoTerminal
pub async fn execute_new_pools(arguments: &serde_json::Value) -> String {
    // Parse arguments
    let network = arguments
        .get("network")
        .and_then(|v| v.as_str())
        .unwrap_or("aptos");

    let page = arguments
        .get("page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .min(10) as u32;

    // Construct GeckoTerminal API URL
    let mut url = format!(
        "https://api.geckoterminal.com/api/v2/networks/{}/new_pools?page={}",
        network, page
    );
    url.push_str("&include=base_token,quote_token,dex");

    // Make HTTP request
    let client = reqwest::Client::new();
    let result = match client
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let result = format_new_pools_response(&data, network);
                        // Ensure we never return an empty string to prevent Telegram error
                        if result.trim().is_empty() {
                            format!(
                                "✨ No new pools found for {} network. The API returned valid data but no pools matched the criteria.",
                                network
                            )
                        } else {
                            result
                        }
                    }
                    Err(e) => {
                        format!("❌ Error parsing API response: {}", e)
                    }
                }
            } else if response.status() == 404 {
                format!(
                    "❌ Network '{}' not found. Please check the network name and try again.",
                    network
                )
            } else if response.status() == 429 {
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            } else {
                format!(
                    "❌ API request failed with status: {} - {}",
                    response.status(),
                    response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string())
                )
            }
        }
        Err(e) => {
            format!("❌ Network error when calling GeckoTerminal API: {}", e)
        }
    };

    // Final safety check to prevent empty responses
    if result.trim().is_empty() {
        format!(
            "🔧 Debug: Function completed but result was empty. Network: {}, URL attempted: {}",
            network, url
        )
    } else {
        result
    }
}

/// Format the new pools API response into a readable string
fn format_new_pools_response(data: &serde_json::Value, network: &str) -> String {
    let mut result = format!("✨ **Newest Pools on {}**\n\n", network.to_uppercase());

    // Build lookup maps for tokens and DEXes from included array
    let mut token_map = std::collections::HashMap::new();
    let mut dex_map = std::collections::HashMap::new();
    if let Some(included) = data.get("included").and_then(|d| d.as_array()) {
        for item in included {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                match item.get("type").and_then(|v| v.as_str()) {
                    Some("token") => {
                        token_map.insert(id, item);
                    }
                    Some("dex") => {
                        dex_map.insert(id, item);
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(pools) = data.get("data").and_then(|d| d.as_array()) {
        if pools.is_empty() {
            result.push_str("No new pools found for this network.\n");
        } else {
            for (index, pool) in pools.iter().enumerate() {
                if let Some(attributes) = pool.get("attributes") {
                    let name = attributes
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown Pool");
                    let pool_address = attributes
                        .get("address")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let pool_created_at = attributes
                        .get("pool_created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");
                    let reserve_usd = attributes
                        .get("reserve_in_usd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    let base_token_price = attributes
                        .get("base_token_price_usd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");

                    let main_change_24h = attributes
                        .get("price_change_percentage")
                        .and_then(|v| v.get("h24"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("0");
                    let price_change_formatted = if let Ok(change) = main_change_24h.parse::<f64>()
                    {
                        if change >= 0.0 {
                            format!("📈 +{:.2}%", change)
                        } else {
                            format!("📉 {:.2}%", change)
                        }
                    } else {
                        "➡️ 0.00%".to_string()
                    };

                    let liquidity_formatted = format_large_number(reserve_usd);
                    let base_price_formatted = format_price(base_token_price);

                    let created_date = if pool_created_at != "Unknown" {
                        pool_created_at.split('T').next().unwrap_or(pool_created_at)
                    } else {
                        "Unknown"
                    };

                    // --- ENRICH WITH TOKEN & DEX INFO ---
                    let (base_token_info, quote_token_info, dex_info) =
                        if let Some(relationships) = pool.get("relationships") {
                            let base_token_id = relationships
                                .get("base_token")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            let quote_token_id = relationships
                                .get("quote_token")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            let dex_id = relationships
                                .get("dex")
                                .and_then(|r| r.get("data"))
                                .and_then(|d| d.get("id"))
                                .and_then(|v| v.as_str());
                            (
                                base_token_id.and_then(|id| token_map.get(id)),
                                quote_token_id.and_then(|id| token_map.get(id)),
                                dex_id.and_then(|id| dex_map.get(id)),
                            )
                        } else {
                            (None, None, None)
                        };

                    let (_, base_symbol) = if let Some(token) = base_token_info {
                        let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                        (
                            attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                        )
                    } else {
                        ("?", "?")
                    };
                    let (_, quote_symbol) = if let Some(token) = quote_token_info {
                        let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                        (
                            attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                            attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                        )
                    } else {
                        ("?", "?")
                    };
                    let dex_name = if let Some(dex) = dex_info {
                        dex.get("attributes")
                            .and_then(|a| a.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown DEX")
                    } else {
                        attributes
                            .get("dex_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown DEX")
                    };

                    result.push_str(&format!(
                        "**{}. {} ({})** {}\n\
🔹 **Pair:** {} / {}\n\
🏦 **DEX:** {}\n\
💰 **Price:** ${}\n\
💧 **Liquidity:** ${}\n\
📅 **Created:** {}\n\
🔗 [View on GeckoTerminal](https://www.geckoterminal.com/{}/pools/{})\n\n",
                        index + 1,
                        name,
                        dex_name,
                        price_change_formatted,
                        base_symbol,
                        quote_symbol,
                        dex_name,
                        base_price_formatted,
                        liquidity_formatted,
                        created_date,
                        network,
                        pool_address
                    ));
                }
            }
            result.push_str(&format!(
                "📈 Data from GeckoTerminal • Showing {}/{} pools",
                pools.len(),
                pools.len()
            ));
        }
    } else {
        result.push_str("❌ No pool data found in API response.");
    }
    result
}

/// Execute get time fetch from WorldTimeAPI
pub async fn execute_get_time(arguments: &serde_json::Value) -> String {
    log::info!("Executing get time tool");
    log::info!("Arguments: {:?}", arguments);

    let timezone = arguments
        .get("timezone")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Africa/Dakar");

    // Use TIME_API_BASE_URL from env if set, otherwise default to just the base
    let base_url =
        std::env::var("TIME_API_BASE_URL").unwrap_or_else(|_| "https://timeapi.io/api".to_string());
    let url = format!("{}/Time/current/zone?timeZone={}", base_url, timezone);

    let client = reqwest::Client::new();
    match client
        .get(&url)
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => format_time_response_timeapi(&data),
                    Err(e) => {
                        log::error!("Failed to parse time API response: {}", e);
                        format!("❌ Error parsing time API response: {}", e)
                    }
                }
            } else {
                format!(
                    "❌ Error fetching time for timezone '{}'. Please check the timezone name (e.g., 'Europe/London').",
                    timezone
                )
            }
        }
        Err(e) => {
            log::error!("Network error when calling timeapi.io: {}", e);
            format!("❌ Network error when calling timeapi.io: {}", e)
        }
    }
}

/// Helper for formatting timeapi.io response
fn format_time_response_timeapi(data: &serde_json::Value) -> String {
    let timezone = data
        .get("timeZone")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let date = data.get("date").and_then(|v| v.as_str()).unwrap_or("");
    let time = data.get("time").and_then(|v| v.as_str()).unwrap_or("");
    log::info!("Time: {}", time);
    log::info!("Date: {}", date);
    let day_of_week = data.get("dayOfWeek").and_then(|v| v.as_str()).unwrap_or("");
    let dst_active = data
        .get("dstActive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if time.is_empty() {
        log::error!("Could not extract time from timeapi.io response");
        return "❌ Could not extract the time from the API response.".to_string();
    }

    // Get current UTC epoch seconds for DAO calculations
    // Since we're requesting UTC time from the API, we can use current timestamp
    let epoch_seconds = Utc::now().timestamp() as u64;

    format!(
        "🕰️ The current time in **{}** is **{}** on **{}** (Date: {}, DST: {}).\n\n**EPOCH SECONDS: {}** (Use this value for DAO date calculations)",
        timezone,
        time,
        day_of_week,
        date,
        if dst_active { "active" } else { "inactive" },
        epoch_seconds
    )
}

/// Execute Fear & Greed Index fetch from Alternative.me
pub async fn execute_fear_and_greed_index(arguments: &serde_json::Value) -> String {
    let limit = arguments.get("days").and_then(|v| v.as_u64()).unwrap_or(1);

    // Use date_format=world to get DD-MM-YYYY dates instead of unix timestamps
    let url = format!(
        "https://api.alternative.me/fng/?limit={}&date_format=world",
        limit
    );

    let client = reqwest::Client::new();
    match client
        .get(&url)
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => format_fear_and_greed_response(&data),
                    Err(e) => {
                        log::error!("Failed to parse Fear & Greed API response: {}", e);
                        format!("❌ Error parsing Fear & Greed API response: {}", e)
                    }
                }
            } else {
                format!(
                    "❌ Error fetching Fear & Greed Index. Status: {}",
                    response.status()
                )
            }
        }
        Err(e) => {
            log::error!("Network error when calling Fear & Greed API: {}", e);
            format!("❌ Network error when calling Fear & Greed API: {}", e)
        }
    }
}

/// Format the Fear & Greed Index API response into a readable string
fn format_fear_and_greed_response(data: &serde_json::Value) -> String {
    if let Some(index_data_array) = data.get("data").and_then(|d| d.as_array()) {
        if index_data_array.is_empty() {
            log::error!("No Fear & Greed Index data found in API response");
            return "❌ No Fear & Greed Index data could be found.".to_string();
        }

        // Handle single-day response (latest)
        if index_data_array.len() == 1 {
            let index_data = &index_data_array[0];
            let value = index_data
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("N/A");
            let classification = index_data
                .get("value_classification")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let time_until_update = index_data
                .get("time_until_update")
                .and_then(|v| v.as_str())
                .unwrap_or("0");

            let emoji = match classification {
                "Extreme Fear" => "😨",
                "Fear" => "😟",
                "Neutral" => "😐",
                "Greed" => "😊",
                "Extreme Greed" => "🤑",
                _ => "📊",
            };

            let hours_until_update = time_until_update.parse::<f64>().unwrap_or(0.0) / 3600.0;

            return format!(
                "**Crypto Market Sentiment: Fear & Greed Index**\n\n\
                {} **{} - {}**\n\n\
                The current sentiment in the crypto market is **{}**.\n\
                *Next update in {:.1} hours.*",
                emoji, value, classification, classification, hours_until_update
            );
        } else {
            // Handle historical data response
            let mut result = format!(
                "**Fear & Greed Index - Last {} Days**\n\n",
                index_data_array.len()
            );
            for item in index_data_array {
                let value = item.get("value").and_then(|v| v.as_str()).unwrap_or("N/A");
                let classification = item
                    .get("value_classification")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let date_str = item
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Date");

                let emoji = match classification {
                    "Extreme Fear" => "😨",
                    "Fear" => "😟",
                    "Neutral" => "😐",
                    "Greed" => "😊",
                    "Extreme Greed" => "🤑",
                    _ => "📊",
                };

                result.push_str(&format!(
                    "{} **{}**: {} ({})\n",
                    emoji, date_str, value, classification
                ));
            }
            return result;
        }
    } else {
        log::error!("❌ Could not retrieve Fear & Greed Index data from the API response");
        "❌ Could not retrieve Fear & Greed Index data from the API response.".to_string()
    }
}

pub async fn execute_pay_users(
    arguments: &serde_json::Value,
    bot: Bot,
    msg: Message,
    bot_deps: BotDependencies,
    group_id: Option<String>,
) -> String {
    let mut version = CoinVersion::V1;

    let amount = arguments
        .get("amount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let symbol = arguments
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("APT");
    let empty_vec = Vec::new();
    let users_array = arguments
        .get("users")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_vec);

    let users = users_array
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    let (token_type, decimals) =
        if symbol.to_lowercase() == "apt" || symbol.to_lowercase() == "aptos" {
            version = CoinVersion::V1;
            ("0x1::aptos_coin::AptosCoin".to_string(), 8u8)
        } else {
            let token = bot_deps.panora.get_token_by_symbol(symbol).await;

            if token.is_err() {
                log::error!("❌ Error getting token: {}", token.as_ref().err().unwrap());
                return format!("❌ Error getting token: {}", token.err().unwrap());
            }

            let token = token.unwrap();

            let token_type_result = if token.token_address.as_ref().is_some() {
                token.token_address.as_ref().unwrap().to_string()
            } else {
                version = CoinVersion::V2;
                token.fa_address.clone()
            };

            (token_type_result, token.decimals)
        };

    // Convert amount to blockchain format using token decima
    let blockchain_amount = (amount as f64 * 10_f64.powi(decimals as i32)) as u64;

    let user_addresses = users
        .iter()
        .map(|u| {
            let user_data = bot_deps.auth.get_credentials(u.as_str());

            if user_data.is_none() {
                log::error!("❌ User not found");
                return None;
            }

            user_data
        })
        .filter(|u| u.is_some())
        .map(|u| u.unwrap().resource_account_address)
        .collect::<Vec<_>>();

    if user_addresses.is_empty() {
        log::error!("❌ No users found");
        return "❌ No users found".to_string();
    }

    // Calculate per-user amount for display
    let per_user_amount = amount / users.len() as f64;

    // Get user ID early to avoid moved value issues
    let user_id = if let Some(user) = &msg.from {
        user.id.0 as i64
    } else {
        log::error!("❌ Could not get user ID");
        return "❌ Could not get user ID".to_string();
    };

    // Get JWT token and determine if it's a group transfer
    let (jwt_token, is_group_transfer) = if group_id.is_some() {
        let admin_ids = bot.get_chat_administrators(msg.chat.id).await;

        if admin_ids.is_err() {
            log::error!(
                "❌ Error getting chat administrators: {}",
                admin_ids.err().unwrap()
            );
            return "❌ Error getting chat administrators".to_string();
        }

        let admin_ids = admin_ids.unwrap();

        let is_admin = admin_ids
            .iter()
            .any(|admin| admin.user.id.to_string() == user_id.to_string());

        if !is_admin {
            log::error!("❌ User is not an admin");
            return "❌ Only admins can send tokens to members".to_string();
        }

        let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

        if group_credentials.is_none() {
            log::error!("❌ Group not found");
            return "❌ Group not found".to_string();
        }

        (group_credentials.unwrap().jwt, true)
    } else {
        let user = msg.from;

        if user.is_none() {
            log::error!("❌ User not found");
            return "❌ User not found".to_string();
        }

        let user = user.unwrap();

        let username = user.username;

        if username.is_none() {
            log::error!("❌ Username not found");
            return "❌ Username not found".to_string();
        }

        let username = username.unwrap();

        let user_credentials = bot_deps.auth.get_credentials(&username);

        if user_credentials.is_none() {
            log::error!("❌ User not found");
            return "❌ User not found".to_string();
        }

        (user_credentials.unwrap().jwt, false)
    };

    // Create pending transaction with 1 minute expiration and unique base64-encoded UUID
    let now = Utc::now().timestamp() as u64;
    let expires_at = now + 60; // 1 minute from now
    let transaction_id = {
        use base64::Engine;
        base64::prelude::BASE64_STANDARD.encode(uuid::Uuid::new_v4().as_bytes())
    };

    let pending_transaction = PendingTransaction {
        transaction_id,
        amount: blockchain_amount,
        users: user_addresses.clone(),
        coin_type: token_type,
        version,
        jwt_token,
        is_group_transfer,
        symbol: symbol.to_string(),
        user_addresses,
        original_usernames: users.clone(),
        per_user_amount,
        created_at: now,
        expires_at,
        chat_id: msg.chat.id.0, // Store the chat ID from the message
        message_id: 0,          // Placeholder - will be updated after message is sent
    };

    // Convert group_id from Option<String> to Option<i64>
    let group_id_i64 = group_id.and_then(|gid| gid.parse::<i64>().ok());

    // Store the pending transaction (includes internal verification)
    if let Err(e) = bot_deps.pending_transactions.set_pending_transaction(
        user_id,
        group_id_i64,
        &pending_transaction,
    ) {
        log::error!("❌ Failed to store pending transaction: {}", e);
        return "❌ Failed to prepare transaction".to_string();
    }

    log::info!(
        "✅ Pending transaction stored successfully with ID: {}",
        pending_transaction.transaction_id
    );

    // Return summary for AI to incorporate
    format!(
        "Confirm sending {:.2} {} total, split evenly among {} users ({:.2} each).",
        amount,
        symbol,
        users.len(),
        per_user_amount
    )
}

pub async fn execute_get_wallet_address(
    msg: Message,
    bot_deps: BotDependencies,
    group_id: Option<String>,
) -> String {
    let user = msg.from;

    if user.is_none() {
        log::error!("❌ User not found");
        return "❌ User not found".to_string();
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        log::error!("❌ Username not found");
        return "❌ Username not found".to_string();
    }

    let username = username.unwrap();

    let resource_account_address = if group_id.is_some() {
        let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

        if group_credentials.is_none() {
            log::error!("❌ Group not found");
            return "❌ Group not found".to_string();
        }

        let group_credentials = group_credentials.unwrap();

        group_credentials.resource_account_address
    } else {
        let user_credentials = bot_deps.auth.get_credentials(&username);

        if user_credentials.is_none() {
            log::error!("❌ User not found");
            return "❌ User not found".to_string();
        }

        user_credentials.unwrap().resource_account_address
    };

    resource_account_address
}

pub async fn execute_get_balance(
    arguments: &serde_json::Value,
    msg: Message,
    group_id: Option<String>,
    bot_deps: BotDependencies,
) -> String {
    let resource_account_address = if group_id.is_some() {
        let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

        if group_credentials.is_none() {
            log::error!("❌ Group not found");
            return "❌ Group not found".to_string();
        }

        group_credentials.unwrap().resource_account_address
    } else {
        let user = msg.from;

        if user.is_none() {
            log::error!("❌ User not found");
            return "❌ User not found".to_string();
        }

        let user = user.unwrap();

        let username = user.username;

        if username.is_none() {
            log::error!("❌ Username not found");
            return "❌ Username not found".to_string();
        }

        let username = username.unwrap();

        bot_deps
            .auth
            .get_credentials(&username)
            .unwrap()
            .resource_account_address
    };

    let symbol = arguments
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("APT");

    let (token_type, decimals, token_symbol) =
        if symbol.to_lowercase() == "apt" || symbol.to_lowercase() == "aptos" {
            (
                "0x1::aptos_coin::AptosCoin".to_string(),
                8u8,
                "APT".to_string(),
            )
        } else {
            let tokens = bot_deps.panora.get_token_by_symbol(symbol).await;

            if tokens.is_err() {
                log::error!("❌ Error getting token: {}", tokens.as_ref().err().unwrap());
                return format!("❌ Error getting token: {}", tokens.err().unwrap());
            }

            let token = tokens.unwrap();

            let token_type = if token.token_address.as_ref().is_some() {
                token.token_address.as_ref().unwrap().to_string()
            } else {
                token.fa_address.clone()
            };

            (token_type, token.decimals, token.symbol.clone())
        };

    let balance = bot_deps
        .panora
        .aptos
        .node
        .get_account_balance(resource_account_address, token_type.to_string())
        .await;

    if balance.is_err() {
        log::error!(
            "❌ Error getting balance: {}",
            balance.as_ref().err().unwrap()
        );
        return format!("❌ Error getting balance: {}", balance.err().unwrap());
    }

    let raw_balance = balance.unwrap().into_inner();

    let balance_i64 = raw_balance.as_i64();

    if balance_i64.is_none() {
        log::error!("❌ Balance not found");
        return "❌ Balance not found".to_string();
    }

    let raw_balance = balance_i64.unwrap();

    // Convert raw balance to human readable format using decimals
    let human_balance = raw_balance as f64 / 10_f64.powi(decimals as i32);

    println!(
        "Raw balance: {}, Human balance: {}",
        raw_balance, human_balance
    );

    format!("💰 <b>Balance</b>: {:.6} {}", human_balance, token_symbol)
}

pub async fn execute_withdraw_funds(
    arguments: &serde_json::Value,
    msg: Message,
    bot_deps: BotDependencies,
) -> String {
    let app_url = env::var("APP_URL");

    if app_url.is_err() {
        return "❌ APP_URL not found".to_string();
    }

    let app_url = app_url.unwrap();

    let chat = msg.chat;

    if chat.is_group() || chat.is_supergroup() || !chat.is_private() || chat.is_channel() {
        return "❌ This command is only available in private chats".to_string();
    }

    let user = msg.from;

    if user.is_none() {
        return "❌ User not found".to_string();
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        return "❌ Username not found".to_string();
    }

    let username = username.unwrap();

    let user_credentials = bot_deps.auth.get_credentials(&username);

    if user_credentials.is_none() {
        return "❌ User not found".to_string();
    }

    let user_credentials = user_credentials.unwrap();

    let symbol = arguments
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("APT");

    let amount = arguments
        .get("amount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let tokens = bot_deps.panora.get_panora_token_list().await;

    if tokens.is_err() {
        let error_msg = tokens.as_ref().err().unwrap().to_string();
        log::error!("❌ Error getting token list: {}", error_msg);

        // Handle rate limiting specifically
        if error_msg.contains("429")
            || error_msg.contains("rate limit")
            || error_msg.contains("Too Many Requests")
        {
            return "⚠️ Panora API is currently experiencing high demand. Please wait a moment and try again.".to_string();
        }

        return format!("❌ Error getting token list: {}", error_msg);
    }

    let tokens = tokens.unwrap();

    let token = tokens
        .iter()
        .find(|t| t.symbol.to_lowercase() == symbol.to_lowercase());

    if token.is_none() {
        return "❌ Token not found".to_string();
    }

    let token = token.unwrap();

    let token_type = if token.token_address.as_ref().is_some() {
        token.token_address.as_ref().unwrap().to_string()
    } else {
        token.fa_address.clone()
    };

    let balance = bot_deps
        .panora
        .aptos
        .node
        .get_account_balance(
            user_credentials.resource_account_address,
            token_type.to_string(),
        )
        .await;

    if balance.is_err() {
        return "❌ Error getting balance".to_string();
    }

    let balance = balance.unwrap().into_inner();

    let balance_i64 = balance.as_i64();

    if balance_i64.is_none() {
        return "❌ Balance not found".to_string();
    }

    let balance_i64 = balance_i64.unwrap();

    if balance_i64 < amount as i64 {
        return "❌ Insufficient balance".to_string();
    }

    let url = format!("{}/withdraw?coin={}&amount={}", app_url, symbol, amount);

    url
}

pub async fn execute_fund_account(
    arguments: &serde_json::Value,
    msg: Message,
    bot_deps: BotDependencies,
) -> String {
    let app_url = env::var("APP_URL");

    if app_url.is_err() {
        return "❌ APP_URL not found".to_string();
    }

    let app_url = app_url.unwrap();

    let chat = msg.chat;

    if chat.is_group() || chat.is_supergroup() || !chat.is_private() || chat.is_channel() {
        return "❌ This command is only available in private chats".to_string();
    }

    let user = msg.from;

    if user.is_none() {
        return "❌ User not found".to_string();
    }

    let user = user.unwrap();

    let username = user.username;

    if username.is_none() {
        return "❌ Username not found".to_string();
    }

    let username = username.unwrap();

    let user_credentials = bot_deps.auth.get_credentials(&username);

    if user_credentials.is_none() {
        return "❌ User not found".to_string();
    }

    let user_credentials = user_credentials.unwrap();

    let symbol = arguments
        .get("symbol")
        .and_then(|v| v.as_str())
        .unwrap_or("APT");

    let amount = arguments
        .get("amount")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let tokens = bot_deps.panora.get_panora_token_list().await;

    if tokens.is_err() {
        return "❌ Error getting token list".to_string();
    }

    let tokens = tokens.unwrap();

    let token = tokens
        .iter()
        .find(|t| t.symbol.to_lowercase() == symbol.to_lowercase());

    if token.is_none() {
        return "❌ Token not found".to_string();
    }

    let token = token.unwrap();

    let token_type = if token.token_address.as_ref().is_some() {
        token.token_address.as_ref().unwrap().to_string()
    } else {
        token.fa_address.clone()
    };

    // Get balance from user's main wallet (not resource account)
    let balance = bot_deps
        .panora
        .aptos
        .node
        .get_account_balance(user_credentials.account_address, token_type.to_string())
        .await;

    if balance.is_err() {
        return "❌ Error getting balance".to_string();
    }

    let balance = balance.unwrap().into_inner();

    let balance_i64 = balance.as_i64();

    if balance_i64.is_none() {
        return "❌ Balance not found".to_string();
    }

    let balance_i64 = balance_i64.unwrap();

    if balance_i64 < amount as i64 {
        return "❌ Insufficient balance".to_string();
    }

    let url = format!("{}/fund?coin={}&amount={}", app_url, symbol, amount);

    url
}

/// Execute prices command to display model pricing information
pub async fn execute_prices(_arguments: &serde_json::Value) -> String {
    "💰 <b>Model Prices</b> <i>(per 1000 tokens)</i>

🤖 <b>AI Models:</b>
• <code>gpt-5</code> - <b>$0.00410</b>
• <code>gpt-5-mini</code> - <b>$0.00082</b>
• <code>gpt-5-nano (sentinel)</code> - <b>$0.00016</b>

🛠️ <b>Tools:</b>
• <code>FileSearch</code> - <b>$0.0040</b>
• <code>ImageGeneration</code> - <b>$0.16</b>
• <code>WebSearchPreview</code> - <b>$0.0160</b>

💳 <b>Payment Information:</b>
💰 Payment is made in <b>your selected payment token (deafult APT)</b> at the <u>dollar market rate</u>
⚠️ <i>All prices are subject to change based on provider rates</i>"
        .to_string()
}

/// Fetch the recent messages from the rolling buffer (up to 30 lines)
pub async fn execute_get_recent_messages(msg: Message, bot_deps: BotDependencies) -> String {
    if msg.chat.is_private() {
        return "This tool is only available in group chats.".into();
    }

    let lines = fetch(msg.chat.id, bot_deps.history_storage.clone()).await;
    if lines.is_empty() {
        return "(No recent messages stored.)".into();
    }

    lines
        .into_iter()
        .map(|e| match e.sender {
            Some(name) => format!("{name}: {}", e.text),
            None => e.text,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Core helper for schedules: fetch recent messages by ChatId (no Message required)
pub async fn execute_get_recent_messages_for_chat(
    chat_id: ChatId,
    bot_deps: BotDependencies,
) -> String {
    let lines = fetch(chat_id, bot_deps.history_storage.clone()).await;
    if lines.is_empty() {
        return "(No recent messages stored.)".into();
    }

    lines
        .into_iter()
        .map(|e| match e.sender {
            Some(name) => format!("{name}: {}", e.text),
            None => e.text,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
