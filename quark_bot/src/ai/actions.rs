use std::env;

use chrono::Utc;
use reqwest::StatusCode;
use quark_core::helpers::dto::CoinVersion;
use teloxide::Bot;
use teloxide::prelude::Requester;
use teloxide::types::{ChatId, Message};
use tokio::time::{sleep, Duration};

use crate::dependencies::BotDependencies;
use crate::message_history::handler::fetch;
use crate::pending_transactions::dto::PendingTransaction;
use crate::ai::{
    GeckoRequestError, GeckoPayloadShape, GeckoPayloadState,
    GECKO_MAX_RETRIES, GECKO_RETRY_BASE_DELAY_MS,
};


/// Execute a GeckoTerminal request with retry/backoff logic and return the parsed JSON body.
async fn send_gecko_request(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, GeckoRequestError> {
    let mut attempt = 0;
    loop {
        let attempt_number = attempt + 1;
        match client
            .get(url)
            .header("Accept", "application/json")
            .header("User-Agent", "QuarkBot/1.0")
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let body = match response.text().await {
                    Ok(body) => body,
                    Err(error) => {
                        if attempt_number >= GECKO_MAX_RETRIES {
                            log::error!(
                                "Failed to read GeckoTerminal response after {} attempts: {} (url: {})",
                                attempt_number,
                                error,
                                url
                            );
                            return Err(GeckoRequestError::ResponseRead(error));
                        }

                        let delay =
                            Duration::from_millis(GECKO_RETRY_BASE_DELAY_MS * attempt_number as u64);
                        log::warn!(
                            "Reading GeckoTerminal response failed on attempt {}: {}. Retrying in {}ms...",
                            attempt_number,
                            error,
                            delay.as_millis()
                        );
                        sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                };

                if !status.is_success() {
                    let should_retry =
                        status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS;
                    if should_retry && attempt_number < GECKO_MAX_RETRIES {
                        let delay =
                            Duration::from_millis(GECKO_RETRY_BASE_DELAY_MS * attempt_number as u64);
                        log::warn!(
                            "GeckoTerminal request returned status {} on attempt {}. Retrying in {}ms...",
                            status,
                            attempt_number,
                            delay.as_millis()
                        );
                        sleep(delay).await;
                        attempt += 1;
                        continue;
                    }

                    log::error!(
                        "GeckoTerminal request failed with status {} after {} attempts (url: {}): {}",
                        status,
                        attempt_number,
                        url,
                        body
                    );
                    return Err(GeckoRequestError::Http { status, body });
                }

                match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(json) => {
                        if let Some(errors) = json.get("errors").and_then(|v| v.as_array()) {
                            if !errors.is_empty() {
                                let messages: Vec<String> = errors
                                    .iter()
                                    .map(|error| {
                                        error
                                            .get("detail")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string())
                                            .or_else(|| {
                                                error
                                                    .get("title")
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string())
                                            })
                                            .unwrap_or_else(|| error.to_string())
                                    })
                                    .collect();
                                if attempt_number >= GECKO_MAX_RETRIES {
                                    log::error!(
                                        "GeckoTerminal API returned error payload after {} attempts (url: {}): {:?}",
                                        attempt_number,
                                        url,
                                        messages
                                    );
                                    return Err(GeckoRequestError::Api(messages));
                                }

                                let delay =
                                    Duration::from_millis(GECKO_RETRY_BASE_DELAY_MS * attempt_number as u64);
                                log::warn!(
                                    "GeckoTerminal API returned error payload on attempt {}: {:?}. Retrying in {}ms...",
                                    attempt_number,
                                    messages,
                                    delay.as_millis()
                                );
                                sleep(delay).await;
                                attempt += 1;
                                continue;
                            }
                        }

                        return Ok(json);
                    }
                    Err(error) => {
                        if attempt_number >= GECKO_MAX_RETRIES {
                            log::error!(
                                "Failed to parse GeckoTerminal JSON after {} attempts: {} (url: {})",
                                attempt_number,
                                error,
                                url
                            );
                            return Err(GeckoRequestError::Parse(error));
                        }

                        let delay =
                            Duration::from_millis(GECKO_RETRY_BASE_DELAY_MS * attempt_number as u64);
                        log::warn!(
                            "Parsing GeckoTerminal payload failed on attempt {}: {}. Retrying in {}ms...",
                            attempt_number,
                            error,
                            delay.as_millis()
                        );
                        sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                }
            }
            Err(error) => {
                if attempt_number >= GECKO_MAX_RETRIES {
                    log::error!(
                        "GeckoTerminal request failed after {} attempts: {} (url: {})",
                        attempt_number,
                        error,
                        url
                    );
                    return Err(GeckoRequestError::Network(error));
                }

                let delay = Duration::from_millis(GECKO_RETRY_BASE_DELAY_MS * attempt_number as u64);
                log::warn!(
                    "GeckoTerminal request attempt {} failed: {}. Retrying in {}ms...",
                    attempt_number,
                    error,
                    delay.as_millis()
                );
                sleep(delay).await;
            }
        }

        attempt += 1;
    }
}

/// Determine whether the provided GeckoTerminal JSON payload contains usable data for the requested shape.
fn classify_gecko_payload(data: &serde_json::Value, shape: GeckoPayloadShape) -> GeckoPayloadState {
    let Some(payload) = data.get("data") else {
        return GeckoPayloadState::Missing;
    };

    match payload {
        serde_json::Value::Null => GeckoPayloadState::Missing,
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                GeckoPayloadState::Empty
            } else {
                GeckoPayloadState::Populated
            }
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                match shape {
                    GeckoPayloadShape::Collection => GeckoPayloadState::Empty,
                    GeckoPayloadShape::Object => GeckoPayloadState::Missing,
                }
            } else {
                GeckoPayloadState::Populated
            }
        }
        _ => GeckoPayloadState::Populated,
    }
}

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
    let result = match send_gecko_request(&client, &url).await {
        Ok(data) => match classify_gecko_payload(&data, GeckoPayloadShape::Collection) {
            GeckoPayloadState::Populated => match format_trending_pools_response(
                &data,
                network,
                limit,
                duration,
            ) {
                Some(rendered) => rendered,
                None => {
                    log::warn!(
                        "Trending pools payload missing detailed pool data for network {} despite populated classification",
                        network
                    );
                    format!(
                        "❌ GeckoTerminal returned a response without pool data for network '{}'. Please try again shortly.",
                        network
                    )
                }
            },
            GeckoPayloadState::Empty => format!(
                "📊 No trending pools found for {} network. The API returned an empty pool list.",
                network
            ),
            GeckoPayloadState::Missing => {
                log::warn!(
                    "Trending pools API response missing expected data array for network {}",
                    network
                );
                format!(
                    "❌ GeckoTerminal returned a response without pool data for network '{}'. Please try again shortly.",
                    network
                )
            }
        },
        Err(error) => match error {
            GeckoRequestError::Http {
                status: StatusCode::NOT_FOUND,
                ..
            } => {
                log::error!("Network '{}' not found in trending pools API", network);
                format!(
                    "❌ Network '{}' not found. Please check the network name and try again.",
                    network
                )
            }
            GeckoRequestError::Http {
                status: StatusCode::TOO_MANY_REQUESTS,
                ..
            } => {
                log::error!("Rate limit exceeded for trending pools API");
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            }
            GeckoRequestError::Api(messages) => {
                log::error!(
                    "GeckoTerminal API returned errors for trending pools on network {}: {:?}",
                    network,
                    messages
                );
                if messages.is_empty() {
                    "❌ GeckoTerminal returned an error response without details.".to_string()
                } else {
                    format!("❌ GeckoTerminal error: {}", messages.join(" | "))
                }
            }
            other => {
                log::error!(
                    "Network error when calling trending pools GeckoTerminal API after retries: {}",
                    other
                );
                format!(
                    "❌ Network error when calling GeckoTerminal API after retries: {}",
                    other
                )
            }
        },
    };

    result
}

/// Format the trending pools API response into a readable string

fn format_trending_pools_response(
    data: &serde_json::Value,
    network: &str,
    limit: u32,
    duration: &str,
) -> Option<String> {
    let pools = data.get("data").and_then(|d| d.as_array())?;
    if pools.is_empty() {
        return None;
    }

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

    let mut result = format!(
        "🔥 **Trending Pools on {} ({})**\n\n",
        network.to_uppercase(),
        duration
    );

    let mut displayed = 0_usize;
    for pool in pools.iter() {
        if displayed >= limit as usize {
            break;
        }

        let Some(attributes) = pool.get("attributes") else {
            continue;
        };

        displayed += 1;

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

        let price_changes = attributes
            .get("price_change_percentage")
            .map(|pcp| {
                let h1 = pcp.get("h1").and_then(|v| v.as_str()).unwrap_or("0");
                let h6 = pcp.get("h6").and_then(|v| v.as_str()).unwrap_or("0");
                let h24 = pcp.get("h24").and_then(|v| v.as_str()).unwrap_or("0");
                format!("1h: {}% | 6h: {}% | 24h: {}%", h1, h6, h24)
            })
            .unwrap_or_else(|| "No data".to_string());

        let volumes = attributes
            .get("volume_usd")
            .map(|volume| {
                let h5m = volume.get("h5m").and_then(|v| v.as_str()).unwrap_or("0");
                let h1 = volume.get("h1").and_then(|v| v.as_str()).unwrap_or("0");
                let h6 = volume.get("h6").and_then(|v| v.as_str()).unwrap_or("0");
                let h24 = volume.get("h24").and_then(|v| v.as_str()).unwrap_or("0");
                format!(
                    "5m: ${} | 1h: ${} | 6h: ${} | 24h: ${}",
                    format_large_number(h5m),
                    format_large_number(h1),
                    format_large_number(h6),
                    format_large_number(h24)
                )
            })
            .unwrap_or_else(|| "No data".to_string());

        let transactions = attributes
            .get("transactions")
            .map(|txns| {
                let aggregate = |key: &str| {
                    txns.get(key)
                        .map(|window| {
                            let buys = window.get("buys").and_then(|v| v.as_u64()).unwrap_or(0);
                            let sells = window.get("sells").and_then(|v| v.as_u64()).unwrap_or(0);
                            buys + sells
                        })
                        .unwrap_or(0)
                };
                format!(
                    "5m: {} | 1h: {} | 24h: {}",
                    aggregate("h5m"),
                    aggregate("h1"),
                    aggregate("h24")
                )
            })
            .unwrap_or_else(|| "No data".to_string());

        let price_change_formatted = attributes
            .get("price_change_percentage")
            .and_then(|v| v.get("h24"))
            .and_then(|v| v.as_str())
            .and_then(|v| v.parse::<f64>().ok())
            .map(|value| if value >= 0.0 { format!("📈 +{:.2}%", value) } else { format!("📉 {:.2}%", value) })
            .unwrap_or_else(|| "➡️ 0.00%".to_string());

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

        let (base_token_info, quote_token_info, dex_info) = pool
            .get("relationships")
            .map(|relationships| {
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
            })
            .unwrap_or((None, None, None));

        let (base_name, base_symbol, base_addr, base_dec, base_cg) = base_token_info
            .and_then(|token| token.get("attributes"))
            .map(|attr| {
                (
                    attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("decimals")
                        .and_then(|v| v.as_u64())
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    attr.get("coingecko_coin_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-"),
                )
            })
            .unwrap_or(("?", "?", "?", "?".to_string(), "-"));

        let (quote_name, quote_symbol, quote_addr, quote_dec, quote_cg) = quote_token_info
            .and_then(|token| token.get("attributes"))
            .map(|attr| {
                (
                    attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("decimals")
                        .and_then(|v| v.as_u64())
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    attr.get("coingecko_coin_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-"),
                )
            })
            .unwrap_or(("?", "?", "?", "?".to_string(), "-"));

        let dex_name = dex_info
            .and_then(|dex| dex.get("attributes"))
            .and_then(|attr| attr.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                attributes
                    .get("dex_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown DEX")
            });

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
            displayed,
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

    result.push_str("📈 Data from GeckoTerminal • Updates every 30 seconds\n");
    result.push_str(&format!(
        "🌐 Network: {} • Showing {}/{} pools",
        network.to_uppercase(),
        displayed,
        pools.len()
    ));

    Some(result)
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

    let query = &query.replace("$", "");

    let network = match arguments.get("network").and_then(|v| v.as_str()) {
        Some(net) if !net.trim().is_empty() => net,
        _ => {
            log::error!("Pool search called without required network parameter");
            return "❌ Error: 'network' is required for pool search to avoid token confusion.".to_string();
        }
    };

    let page = arguments
        .get("page")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1);

    // Construct GeckoTerminal API URL
    let url = format!(
        "https://api.geckoterminal.com/api/v2/search/pools?query={}&network={}&page={}&include=base_token,quote_token,dex",
        urlencoding::encode(query),
        urlencoding::encode(network),
        page
    );

    // Make HTTP request
    let client = reqwest::Client::new();
    match send_gecko_request(&client, &url).await {
        Ok(data) => match classify_gecko_payload(&data, GeckoPayloadShape::Collection) {
            GeckoPayloadState::Populated => match format_search_pools_response(&data, query, Some(network)) {
                Some(rendered) if !rendered.trim().is_empty() => rendered,
                _ => {
                    log::warn!(
                        "Search pools payload missing rendered content. Query: {} | Network: {}",
                        query,
                        network
                    );
                    format!(
                        "❌ GeckoTerminal returned a response without pool data for query '{}' on network '{}'.",
                        query,
                        network
                    )
                }
            },
            GeckoPayloadState::Empty => format!(
                "🔍 No pools found for '{}' on '{}'.",
                query,
                network
            ),
            GeckoPayloadState::Missing => {
                log::warn!(
                    "Search pools API response missing expected data array. Query: {} | Network: {}",
                    query,
                    network
                );
                format!(
                    "❌ GeckoTerminal returned a response without pool data for query '{}' on network '{}'. Please try again shortly.",
                    query,
                    network
                )
            }
        },
        Err(error) => match error {
            GeckoRequestError::Http {
                status: StatusCode::NOT_FOUND,
                ..
            } => {
                log::error!("No pools found for query '{}' (404 response)", query);
                format!("❌ No pools found for query '{}'.", query)
            }
            GeckoRequestError::Http {
                status: StatusCode::TOO_MANY_REQUESTS,
                ..
            } => {
                log::error!("Rate limit exceeded for search pools API");
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            }
            GeckoRequestError::Api(messages) => {
                log::error!(
                    "GeckoTerminal API returned errors for pool search. Query: {} | Network: {} | Errors: {:?}",
                    query,
                    network,
                    messages
                );
                if messages.is_empty() {
                    "❌ GeckoTerminal returned an error response without details.".to_string()
                } else {
                    format!("❌ GeckoTerminal error: {}", messages.join(" | "))
                }
            }
            other => {
                log::error!(
                    "Network error when calling search pools GeckoTerminal API after retries: {}",
                    other
                );
                format!(
                    "❌ Network error when calling GeckoTerminal API after retries: {}",
                    other
                )
            }
        },
    }
}

/// Format the search pools API response into a readable string
fn format_search_pools_response(
    data: &serde_json::Value,
    query: &str,
    network: Option<&str>,
) -> Option<String> {
    let pools = data.get("data").and_then(|d| d.as_array())?;
    if pools.is_empty() {
        return None;
    }

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

    Some(result)
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
    match send_gecko_request(&client, &url).await {
        Ok(data) => match classify_gecko_payload(&data, GeckoPayloadShape::Collection) {
            GeckoPayloadState::Populated => match format_new_pools_response(&data, network) {
                Some(rendered) => rendered,
                None => {
                    log::warn!(
                        "New pools payload missing detailed pool data for network {} despite populated classification",
                        network
                    );
                    format!(
                        "❌ GeckoTerminal returned a response without pool data for network '{}'. Please try again shortly.",
                        network
                    )
                }
            },
            GeckoPayloadState::Empty => format!(
                "✨ No new pools found for {} network. GeckoTerminal reported an empty pool list.",
                network
            ),
            GeckoPayloadState::Missing => {
                log::warn!(
                    "New pools API response missing expected data array for network {}",
                    network
                );
                format!(
                    "❌ GeckoTerminal returned a response without pool data for network '{}'. Please try again shortly.",
                    network
                )
            }
        },
        Err(error) => match error {
            GeckoRequestError::Http {
                status: StatusCode::NOT_FOUND,
                ..
            } => format!(
                "❌ Network '{}' not found. Please check the network name and try again.",
                network
            ),
            GeckoRequestError::Http {
                status: StatusCode::TOO_MANY_REQUESTS,
                ..
            } => {
                "⚠️ Rate limit exceeded. GeckoTerminal allows 30 requests per minute. Please try again later.".to_string()
            }
            GeckoRequestError::Api(messages) => {
                log::error!(
                    "GeckoTerminal API returned errors for new pools on network {}: {:?}",
                    network,
                    messages
                );
                if messages.is_empty() {
                    "❌ GeckoTerminal returned an error response without details.".to_string()
                } else {
                    format!("❌ GeckoTerminal error: {}", messages.join(" | "))
                }
            }
            other => {
                log::error!(
                    "Network error when calling new pools GeckoTerminal API after retries: {}",
                    other
                );
                format!(
                    "❌ Network error when calling GeckoTerminal API after retries: {}",
                    other
                )
            }
        },
    }
}

/// Format the new pools API response into a readable string

fn format_new_pools_response(data: &serde_json::Value, network: &str) -> Option<String> {
    let pools = data.get("data").and_then(|d| d.as_array())?;
    if pools.is_empty() {
        return None;
    }

    let mut result = format!("✨ **Newest Pools on {}**\n\n", network.to_uppercase());

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

            let (quote_name, quote_symbol, quote_addr) = if let Some(token) = quote_token_info {
                let attr = token.get("attributes").unwrap_or(&serde_json::Value::Null);
                (
                    attr.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("symbol").and_then(|v| v.as_str()).unwrap_or("?"),
                    attr.get("address").and_then(|v| v.as_str()).unwrap_or("?"),
                )
            } else {
                ("?", "?", "?")
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

            let created_date = if pool_created_at != "Unknown" {
                pool_created_at.split('T').next().unwrap_or(pool_created_at)
            } else {
                "Unknown"
            };

            let liquidity_formatted = format_large_number(reserve_usd);
            let base_price_formatted = format_price(base_token_price);
            let quote_price_formatted = format_price(quote_token_price);

            result.push_str(&format!(
                "**{}. {}**\n🔹 **DEX:** {}\n🔹 **Base Token:** {} ({})\n  - Address: `{}`\n🔹 **Quote Token:** {} ({})\n  - Address: `{}`\n💰 **Base Price:** ${} | **Quote Price:** ${}\n💧 **Liquidity (USD):** ${}\n📅 **Created:** {}\n🏊 **Pool Address:** `{}`\n🔗 [View on GeckoTerminal](https://www.geckoterminal.com/{}/pools/{})\n\n",
                index + 1,
                name,
                dex_name,
                base_name,
                base_symbol,
                base_addr,
                quote_name,
                quote_symbol,
                quote_addr,
                base_price_formatted,
                quote_price_formatted,
                liquidity_formatted,
                created_date,
                pool_address,
                network,
                pool_address
            ));
        }
    }

    Some(result)
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

/// Execute token price fetch from BitcoinTry
pub async fn execute_price_by_bitcointry(arguments: &serde_json::Value) -> String {
    log::info!("Executing get token price tool");
    log::info!("Arguments: {:?}", arguments);

    // Extract and validate ticker parameter
    let ticker = match arguments.get("ticker").and_then(|v| v.as_str()) {
        Some(t) if !t.trim().is_empty() => t.trim().to_uppercase(),
        _ => {
            log::error!("Token price called without required ticker parameter");
            return "❌ Error: 'ticker' parameter is required (e.g., 'BTC', 'ETH', 'APT').".to_string();
        }
    };

    log::info!("Fetching price for ticker: {}", ticker);

    // Call BitcoinTry API
    let url = "https://api.bitcointry.com/api/v1/summary";
    let client = reqwest::Client::new();

    match client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "QuarkBot/1.0")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let result = format_price_response_by_bitcointry(&data, &ticker);
                        if result.trim().is_empty() {
                            format!(
                                "🔍 No token found with ticker '{}'. Please verify the symbol and try again.",
                                ticker
                            )
                        } else {
                            result
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to parse BitcoinTry API response: {}", e);
                        format!("❌ Error parsing API response: {}", e)
                    }
                }
            } else if response.status() == 404 {
                log::error!("BitcoinTry API returned 404");
                "❌ Token price data not available at this time.".to_string()
            } else if response.status() == 429 {
                log::error!("Rate limit exceeded for BitcoinTry API");
                "⚠️ Rate limit exceeded. Please try again later.".to_string()
            } else {
                let status = response.status();
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                log::error!(
                    "BitcoinTry API request failed with status: {} - {}",
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
            log::error!("Network error when calling BitcoinTry API: {}", e);
            format!("❌ Network error when calling BitcoinTry API: {}", e)
        }
    }
}

/// Format the token price response from BitcoinTry API
fn format_price_response_by_bitcointry(data: &serde_json::Value, ticker: &str) -> String {
    // Parse the array response
    let entries = match data.as_array() {
        Some(arr) => arr,
        None => {
            log::error!("BitcoinTry API did not return an array");
            return "❌ Unexpected API response format.".to_string();
        }
    };

    if entries.is_empty() {
        log::info!("BitcoinTry API returned empty array");
        return format!("🔍 No token data available for '{}'.", ticker);
    }

    log::info!("BitcoinTry API returned {} entries", entries.len());
    
    // Debug: Log first few base_currency values
    for (i, entry) in entries.iter().take(5).enumerate() {
        if let Some(base_curr) = entry.get("base_currency").and_then(|v| v.as_str()) {
            log::info!("Entry {}: base_currency = '{}'", i, base_curr);
        }
    }

    // Filter entries by base_currency (exact uppercase match)
    let mut matches: Vec<&serde_json::Value> = entries
        .iter()
        .filter(|entry| {
            let base_currency = entry
                .get("base_currency")
                .and_then(|v| v.as_str());
            
            if let Some(base_curr) = base_currency {
                let matches = base_curr == ticker;
                log::info!("Comparing '{}' == '{}': {}", base_curr, ticker, matches);
                matches
            } else {
                false
            }
        })
        .collect();

    if matches.is_empty() {
        log::info!("No matches found for ticker: {}", ticker);
        return format!(
            "🔍 No token found with ticker '{}' on BitcoinTry. Please verify the symbol.",
            ticker
        );
    }

    // If multiple matches, select by highest volume
    let best_match = if matches.len() > 1 {
        log::info!("Found {} matches for ticker {}, selecting best by volume", matches.len(), ticker);
        
        // Sort by quote_volume (descending) - the trading volume in USDT
        matches.sort_by(|a, b| {
            let a_vol = a
                .get("quote_volume")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let b_vol = b
                .get("quote_volume")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            b_vol.partial_cmp(&a_vol).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        matches[0]
    } else {
        matches[0]
    };

    // Extract fields from actual API structure
    let base_currency = best_match
        .get("base_currency")
        .and_then(|v| v.as_str())
        .unwrap_or(ticker);
    let quote_currency = best_match
        .get("quote_currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USDT");
    
    // Get trading_pairs or construct it
    let trading_pair_from_api = best_match
        .get("trading_pairs")
        .and_then(|v| v.as_str());
    let trading_pair_default = format!("{}_{}", base_currency, quote_currency);
    let trading_pair = trading_pair_from_api.unwrap_or(&trading_pair_default);
    
    let last_price = best_match
        .get("last_price")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let change_24h = best_match
        .get("price_change_percent_24h")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let base_volume = best_match
        .get("base_volume")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let quote_volume = best_match
        .get("quote_volume")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let highest_bid = best_match
        .get("highest_bid")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let lowest_ask = best_match
        .get("lowest_ask")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let highest_24h = best_match
        .get("highest_price_24h")
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let lowest_24h = best_match
        .get("lowest_price_24h")
        .and_then(|v| v.as_str())
        .unwrap_or("0");

    // Format price change with emoji
    let change_formatted = if let Ok(change) = change_24h.parse::<f64>() {
        if change >= 0.0 {
            format!("📈 +{:.2}%", change)
        } else {
            format!("📉 {:.2}%", change)
        }
    } else {
        "➡️ 0.00%".to_string()
    };

    // Format numbers
    let price_formatted = format_price(last_price);
    let base_volume_formatted = format_large_number(base_volume);
    let quote_volume_formatted = format_large_number(quote_volume);
    let highest_bid_formatted = format_price(highest_bid);
    let lowest_ask_formatted = format_price(lowest_ask);
    let highest_24h_formatted = format_price(highest_24h);
    let lowest_24h_formatted = format_price(lowest_24h);

    // Build result
    format!(
        "💰 **{} / {}** on BitcoinTry\n\n\
        💵 **Price:** ${}\n\
        📊 **24h Change:** {}\n\
        📈 **24h High:** ${}\n\
        📉 **24h Low:** ${}\n\
        💹 **Highest Bid:** ${}\n\
        💹 **Lowest Ask:** ${}\n\
        📊 **24h Volume:** {} {} (${} {})\n\n\
        🔗 Trading Pair: {}",
        base_currency,
        quote_currency,
        price_formatted,
        change_formatted,
        highest_24h_formatted,
        lowest_24h_formatted,
        highest_bid_formatted,
        lowest_ask_formatted,
        base_volume_formatted,
        base_currency,
        quote_volume_formatted,
        quote_currency,
        trading_pair
    )
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
