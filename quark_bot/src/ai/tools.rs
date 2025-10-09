use super::actions::{
    execute_fear_and_greed_index, execute_get_recent_messages, execute_get_time,
    execute_get_wallet_address, execute_new_pools, execute_pay_users, execute_search_pools,
    execute_trending_pools,
};
use crate::{
    ai::actions::{execute_fund_account, execute_get_balance, execute_withdraw_funds},
    dao::handler::execute_create_proposal,
    dependencies::BotDependencies,
};
use open_ai_rust_responses_by_sshift::types::Tool;
use serde_json::json;
use teloxide::{Bot, types::Message};

/// Get account balance tool - returns a Tool for checking user balance
pub fn get_balance_tool() -> Tool {
    Tool::function(
        "get_balance",
        "Get the current account balance for the user. MUST use this tool for all balance check requests. Present the result concisely (e.g., '<b>Balance</b>: <code>{amount}</code> {SYMBOL}'). Do not paste raw JSON; curate the answer. Keep within the overall 4000-character budget and do not add follow-up questions.",
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "The symbol of the token to get the balance for"
                }
            },
            "required": ["symbol"],
            "additionalProperties": false
        }),
    )
}

pub fn get_wallet_address_tool() -> Tool {
    Tool::function(
        "get_wallet_address",
        "Get the current wallet address for the user. MUST use this tool for all wallet address check requests. Present the address in <code>...</code>. Do not paste raw JSON; keep concise and avoid follow-up questions.",
        json!({}),
    )
}

/// Withdraw funds tool - returns a Tool for withdrawing funds
pub fn withdraw_funds_tool() -> Tool {
    Tool::function(
        "withdraw_funds",
        "Withdraw funds from the user's account. Strictly follow the protocol described in this tool's description. Always include the provided URL as a clickable link (e.g., '<a href=\"URL\">Withdraw funds</a>'). Present concisely and avoid follow-up questions.",
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "The symbol of the token to withdraw"
                },
                "amount": {
                    "type": "number",
                    "description": "The amount of coins to withdraw"
                }
            },
            "required": ["symbol", "amount"],
            "additionalProperties": false
        }),
    )
}

/// Fund account tool - returns a Tool for funding the resource account
pub fn fund_account_tool() -> Tool {
    Tool::function(
        "fund_account",
        "Fund the user's resource account with tokens from their main wallet. Strictly follow the protocol described in this tool's description. Always include the provided URL as a clickable link (e.g., '<a href=\"URL\">Fund account</a>'). Present concisely and avoid follow-up questions.",
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "The symbol of the token to fund"
                },
                "amount": {
                    "type": "number",
                    "description": "The amount of coins to fund"
                }
            },
            "required": ["symbol", "amount"],
            "additionalProperties": false
        }),
    )
}

/// Get trending pools tool - returns a Tool for fetching trending DEX pools on a specific blockchain
pub fn get_trending_pools_tool() -> Tool {
    Tool::function(
        "get_trending_pools",
        "Get the top trending DEX pools on a specific blockchain network from GeckoTerminal. Curate results sized by verbosity (Low: up to 5, Medium: up to 8, High: up to 10). For each item include name/symbol, 1–2 key stats (e.g., 24h change, liquidity), and a link. Do not dump raw JSON.",
        json!({
            "type": "object",
            "properties": {
                "network": {
                    "type": "string",
                    "description": "Blockchain network identifier (e.g., 'aptos' for Aptos, 'eth' for Ethereum, 'bsc' for BSC, 'polygon_pos' for Polygon)",
                    "enum": ["aptos", "sui", "eth", "bsc", "polygon_pos", "avax", "ftm", "cro", "arbitrum", "base", "solana"]
                },
                "limit": {
                    "type": "integer",
                    "description": "Number of trending pools to return (1-20)",
                    "minimum": 1,
                    "maximum": 20,
                    "default": 10
                },
                "page": {
                    "type": "integer",
                    "description": "Page number for pagination (maximum: 10)",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 1
                },
                "duration": {
                    "type": "string",
                    "description": "Duration to sort trending list by",
                    "enum": ["5m", "1h", "6h", "24h"],
                    "default": "24h"
                }
            },
            "required": ["network"],
            "additionalProperties": false
        }),
    )
}

/// Search pools tool - returns a Tool for searching DEX pools by text, ticker, or address
pub fn get_search_pools_tool() -> Tool {
    Tool::function(
        "search_pools",
        "Search for DEX pools on GeckoTerminal by text, token symbol, contract address, or pool address. MANDATORY: Always specify the network parameter to avoid confusion between different tokens with similar names. When multiple pools are returned, prioritize the pool with the highest liquidity (reserve_in_usd) that is paired with either the network's native token (e.g., BNB for BSC, SOL for Solana, ETH for Ethereum) or stablecoins (USDT/USDC). This ensures the most accurate and reliable pricing. Curate results sized by verbosity (Low: up to 5, Medium: up to 8, High: up to 10). For each item include name/symbol, 1–2 key stats, and a link. Do not dump raw JSON.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Free-text search. Accepts a token symbol, token contract address, or a pool address."
                },
                "network": {
                    "type": "string",
                    "description": "REQUIRED: Blockchain network identifier to restrict results to one chain. Prevents confusion between tokens with similar names. Use: 'bsc' for Binance Smart Chain, 'solana' for Solana, 'eth' for Ethereum, 'aptos' for Aptos, 'sui' for Sui, 'base' for Base, etc."
                },
                "page": {
                    "type": "integer",
                    "description": "(Optional) Pagination (20 results per page).",
                    "minimum": 1,
                    "default": 1
                }
            },
            "required": ["query", "network"],
            "additionalProperties": false
        }),
    )
}

/// Get new pools tool - returns a Tool for fetching the latest pools on a specific blockchain
pub fn get_new_pools_tool() -> Tool {
    Tool::function(
        "get_new_pools",
        "Get the latest pools on a specific blockchain network from GeckoTerminal. Curate results sized by verbosity (Low: up to 5, Medium: up to 8, High: up to 10). For each item include name/symbol and a link; avoid low-signal fields. Do not dump raw JSON.",
        json!({
            "type": "object",
            "properties": {
                "network": {
                    "type": "string",
                    "description": "Blockchain network identifier (e.g., 'aptos' for Aptos, 'eth' for Ethereum).",
                    "enum": ["aptos", "sui", "eth", "bsc", "polygon_pos", "avax", "ftm", "cro", "arbitrum", "base", "solana"]
                },
                "page": {
                    "type": "integer",
                    "description": "Page number for pagination (maximum: 10).",
                    "minimum": 1,
                    "maximum": 10,
                    "default": 1
                }
            },
            "required": ["network"],
            "additionalProperties": false
        }),
    )
}

/// Get current time tool - returns a Tool for fetching the current time for a specific timezone
pub fn get_time_tool() -> Tool {
    Tool::function(
        "get_current_time",
        "Get the current time for a specified timezone. CRITICAL: MUST be used before creating any DAO to get the current UTC time for date calculations. Always use timezone 'UTC' for DAO creation.",
        json!({
            "type": "object",
            "properties": {
                "timezone": {
                    "type": "string",
                    "description": "The timezone to get the current time for, in IANA format (e.g., 'America/New_York', 'Europe/London', 'Asia/Tokyo'). Use 'UTC' for DAO creation to ensure consistent time calculations.",
                    "default": "UTC"
                    }
                },
            "required": [],
            "additionalProperties": false
        }),
    )
}

/// Fear & Greed Index tool - returns a Tool for fetching the crypto market sentiment
pub fn get_fear_and_greed_index_tool() -> Tool {
    Tool::function(
        "get_fear_and_greed_index",
        "Retrieve the current or historical Fear & Greed Index for the crypto market. Report as 'Index: NN/100 – {Greed|Fear|Neutral}' plus a 1–2 line interpretation; do not dump raw JSON.",
        json!({
            "type": "object",
            "properties": {
                "days": {
                    "type": "integer",
                    "description": "Number of days of historical data to retrieve (e.g., 7 for the last week). Default is 1 for the latest index.",
                    "minimum": 1,
                    "maximum": 90,
                    "default": 1
                }
            },
            "required": [],
            "additionalProperties": false
        }),
    )
}

/// Get pay users tool - returns a Tool for transferring a specified amount of a selected token to multiple Telegram users by their usernames, with support for different token categories. MUST use this tool for all token send requests.
pub fn get_pay_users_tool() -> Tool {
    Tool::function(
        "get_pay_users",
        "Token Send Tool — Streamlined Protocol\n\nThis tool prepares token transfers to multiple Telegram users by username. The contract automatically splits the total amount evenly among all recipients.\n\nResponse Protocol:\n• After calling this tool, ALWAYS describe the prepared transaction using this exact format:\n  - For individual sends: 'Confirm sending [X] [TOKEN] to [USERNAME].'\n  - For shared sends: 'Confirm sending [X] [TOKEN] total, split evenly among [Y] users ([Z] each).'\n• Do NOT ask for confirmation again - the user will confirm via buttons\n• Do NOT repeat the tool call - execute it once and provide the summary\n• The transaction will be prepared and awaiting user confirmation via inline buttons, YOU WILL now consider the transaction completed since control is now with the user.\n• Do not add closing pleasantries or follow-up questions.\n\n",
        json!({
            "type": "object",
            "properties": {
                "amount": {
                    "type": "number",
                    "description": "The total amount of tokens to send (will be split evenly among all users)"
                },
                "symbol": {
                    "type": "string",
                    "description": "The symbol of the token to send"
                },
                "is_emojicoin": {
                    "type": "boolean",
                    "description": "Only is true if symbol is an emojicoin or input mention it"
                },
                "is_native": {
                    "type": "boolean",
                    "description": "Only is true if input mention is a native token"
                },
                "is_meme": {
                    "type": "boolean",
                    "description": "Only is true if input mention is a meme token"
                },
                "is_bridged": {
                    "type": "boolean",
                    "description": "Only is true if input mention is a bridged token"
                },
                "users": {
                    "type": "array",
                    "description": "telegram usernames without @ for example ['mytestuser', 'mytestuser2']",
                    "items": {
                        "type": "string"
                    }
                }
            },
            "required": ["amount", "symbol", "users"],
            "additionalProperties": false
        }),
    )
}

pub fn create_proposal() -> Tool {
    Tool::function(
        "create_proposal",
        "Create a new voting proposal for the with the given name, description, start date, end date, currency and options to vote for. CRITICAL: You MUST use get_current_time tool with timezone 'UTC' FIRST to get the current time before calling this tool. All dates must be calculated from the current UTC time and converted to seconds since epoch. The symbol parameter is optional - if not provided, the tool will use the saved DAO token preference for the group. If no specific vote duration is mentioned, you can use the saved vote duration preference for the group. If no start time is provided the proposal should start 5 mins from the current time (apply the same rule for 'now'/'immediately'). In your response, show both human-readable UTC times and epoch seconds in <code>...</code>, presented concisely.",
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the proposal"
                },
                "description": {
                    "type": "string",
                    "description": "The description of the proposal"
                },
                "start_date": {
                    "type": "string",
                    "description": "The start date of the proposal in seconds since epoch (UTC+0). MUST be calculated from current UTC time obtained from get_current_time tool. CRITICAL TIME PARSING: Be extremely careful with numbers - '3 minutes' = 180 seconds, '30 minutes' = 1800 seconds. Examples: 'in 1 minute' = current_utc_epoch + 60, 'in 3 minutes' = current_utc_epoch + 180, 'in 5 minutes' = current_utc_epoch + 300, 'in 30 minutes' = current_utc_epoch + 1800, 'in 1 hour' = current_utc_epoch + 3600, 'tomorrow' = current_utc_epoch + 86400. For conflicting times like 'in 5 minutes 29th July 2025', use the relative time (5 minutes from now)."
                },
                "end_date": {
                    "type": "string",
                    "description": "The end date of the proposal in seconds since epoch (UTC+0). Calculate duration from start_date, not from current time. Example: 'end in 3 days' = start_date + 259200 seconds. If no specific duration is mentioned, you can use the saved vote duration preference for this group."
                },
                "options": {
                    "type": "array",
                    "description": "The options to vote for",
                    "items": {
                        "type": "string"
                    }
                },
                "symbol": {
                    "type": "string",
                    "description": "The symbol of the currency of the proposal. Optional - if not provided, will use the saved DAO token preference for this group."
                }
            },
            "required": ["name", "description", "start_date", "end_date", "options"],
            "additionalProperties": false
        }),
    )
}

/// Get recent group messages – returns last ≈30 lines
pub fn get_recent_messages_tool() -> Tool {
    Tool::function(
        "get_recent_messages",
        "Retrieve the most recent messages (up to 30) from this Telegram group chat. Use this tool whenever users ask about: 'what have I missed', 'recent activity', 'what happened', 'group updates', 'catching up', 'conversation history', or use vague references like 'that', 'it', 'what we discussed'. Provide a concise situational summary (key decisions, mentions, dates). Quote selectively using <pre> for short snippets; do not dump raw logs.",
        serde_json::json!({}),
    )
}

/// Execute a custom tool and return the result
pub async fn execute_custom_tool(
    tool_name: &str,
    arguments: &serde_json::Value,
    bot: Bot,
    msg: Message,
    group_id: Option<String>,
    bot_deps: BotDependencies,
) -> String {
    log::info!(
        "Executing tool: {} with arguments: {}",
        tool_name,
        arguments
    );

    let result = match tool_name {
        "get_balance" => execute_get_balance(arguments, msg, group_id, bot_deps.clone()).await,
        "get_wallet_address" => execute_get_wallet_address(msg, bot_deps.clone(), group_id).await,
        "withdraw_funds" => execute_withdraw_funds(arguments, msg, bot_deps.clone()).await,
        "fund_account" => execute_fund_account(arguments, msg, bot_deps.clone()).await,
        "get_trending_pools" => execute_trending_pools(arguments).await,
        "search_pools" => execute_search_pools(arguments).await,
        "get_new_pools" => execute_new_pools(arguments).await,
        "get_current_time" => execute_get_time(arguments).await,
        "get_fear_and_greed_index" => execute_fear_and_greed_index(arguments).await,
        "get_pay_users" => execute_pay_users(arguments, bot, msg, bot_deps.clone(), group_id).await,
        "create_proposal" => {
            execute_create_proposal(arguments, bot, msg, group_id, bot_deps.clone()).await
        }
        "get_recent_messages" => execute_get_recent_messages(msg, bot_deps).await,
        _ => {
            format!("Error: Unknown custom tool '{}'", tool_name)
        }
    };

    log::info!(
        "Tool {} completed with result length: {}",
        tool_name,
        result.len()
    );

    // Ensure we always return a non-empty string
    if result.trim().is_empty() {
        log::warn!(
            "Tool {} returned empty result, providing fallback",
            tool_name
        );
        format!("Tool '{}' executed but returned no output", tool_name)
    } else {
        result
    }
}

pub fn get_all_custom_tools() -> Vec<Tool> {
    vec![
        get_balance_tool(),
        get_wallet_address_tool(),
        withdraw_funds_tool(),
        fund_account_tool(),
        get_trending_pools_tool(),
        get_search_pools_tool(),
        get_new_pools_tool(),
        get_time_tool(),
        get_fear_and_greed_index_tool(),
        get_pay_users_tool(),
        create_proposal(),
        get_recent_messages_tool(),
    ]
}
