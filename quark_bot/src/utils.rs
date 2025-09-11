//! Utility functions for quark_bot.

use chrono::{DateTime, Utc};
use ammonia::Builder as HtmlSanitizerBuilder;
use open_ai_rust_responses_by_sshift::Model;
use quark_core::helpers::dto::{AITool, PurchaseRequest, ToolUsage};
use regex::Regex;
use std::env;
use teloxide::{
    Bot, RequestError,
    prelude::*,
    sugar::request::RequestReplyExt,
    types::{ChatId, InlineKeyboardMarkup, KeyboardMarkup, MessageId, ParseMode, UserId},
};

use crate::dependencies::BotDependencies;

pub enum KeyboardMarkupType {
    InlineKeyboardType(InlineKeyboardMarkup),
    KeyboardType(KeyboardMarkup),
}

/// Helper function to format Unix timestamp into readable date and time
pub fn format_timestamp(timestamp: u64) -> String {
    let datetime = DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_else(|| Utc::now());
    datetime.format("%Y-%m-%d at %H:%M UTC").to_string()
}

/// Helper function to format time duration in a human-readable way
pub fn format_time_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;

    if hours == 0 {
        // Less than 1 hour, show in minutes
        format!("{} minute{}", minutes, if minutes == 1 { "" } else { "s" })
    } else if hours < 24 {
        // 1-23 hours, show in hours
        format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })
    } else {
        // 24+ hours, show in days
        let days = hours / 24;
        format!("{} day{}", days, if days == 1 { "" } else { "s" })
    }
}

/// Get emoji icon based on file extension
pub fn get_file_icon(filename: &str) -> &'static str {
    let extension = filename.split('.').last().unwrap_or("").to_lowercase();
    match extension.as_str() {
        "pdf" => "📄",
        "doc" | "docx" => "📝",
        "xls" | "xlsx" => "📊",
        "ppt" | "pptx" => "📋",
        "txt" | "md" => "📄",
        "jpg" | "jpeg" | "png" | "gif" | "webp" => "🖼️",
        "mp4" | "avi" | "mov" | "mkv" => "🎥",
        "mp3" | "wav" | "flac" | "aac" => "🎵",
        "zip" | "rar" | "7z" => "📦",
        "json" | "xml" | "csv" => "🗂️",
        "py" | "js" | "ts" | "rs" | "cpp" | "java" => "💻",
        _ => "📎",
    }
}

/// Smart filename cleaning and truncation
pub fn clean_filename(filename: &str) -> String {
    // Remove prefixes like "1030814179_" or "group_-1002587813217_"
    let cleaned = if let Some(underscore_pos) = filename.find('_') {
        if filename[..underscore_pos]
            .chars()
            .all(|c| c.is_ascii_digit())
        {
            // Handle user file prefix: "1030814179_filename.pdf" → "filename.pdf"
            &filename[underscore_pos + 1..]
        } else if filename.starts_with("group_") {
            // Handle group file prefix: "group_-1002587813217_filename.pdf" → "filename.pdf"
            if let Some(second_underscore) = filename[6..].find('_') {
                &filename[6 + second_underscore + 1..]
            } else {
                filename
            }
        } else {
            filename
        }
    } else {
        filename
    };
    // Truncate if too long, keeping extension
    if cleaned.len() > 35 {
        if let Some(dot_pos) = cleaned.rfind('.') {
            let name_part = &cleaned[..dot_pos];
            let ext_part = &cleaned[dot_pos..];
            if name_part.len() > 30 {
                format!("{}...{}", &name_part[..27], ext_part)
            } else {
                format!("{}...", &cleaned[..32])
            }
        } else {
            format!("{}...", &cleaned[..32])
        }
    } else {
        cleaned.to_string()
    }
}


// Enhanced markdown to Telegram-HTML converter supporting triple backtick fences and Markdown links
pub fn markdown_to_html(input: &str) -> String {
    // First, convert Markdown links [text](url) to HTML <a href="url">text</a>
    let re_markdown_link = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
    let html_with_links = re_markdown_link.replace_all(input, r#"<a href="$2">$1</a>"#);

    // Clean up redundant URL listings in parentheses that often appear after web search results
    // Pattern: (url1, url2, url3) or (url1; url2) - remove these since we have proper HTML links
    let re_redundant_urls =
        Regex::new(r#"\s*\([^)]*(?:https?://[^\s,;)]+[,\s;]*)+[^)]*\)"#).unwrap();
    let cleaned_html = re_redundant_urls.replace_all(&html_with_links, "");

    // Handle fenced code blocks ```lang\n...\n```
    let mut html = String::new();
    let mut lines = cleaned_html.lines();
    let mut in_code = false;
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            if !in_code {
                in_code = true;
                html.push_str("<pre>");
            } else {
                in_code = false;
                html.push_str("</pre>\n");
            }
            continue;
        }
        if in_code {
            // Only escape within code blocks
            html.push_str(&teloxide::utils::html::escape(line));
            html.push('\n');
        } else {
            // Outside code blocks, preserve the line as-is (may contain HTML)
            html.push_str(line);
            html.push('\n');
        }
    }
    html
}

/// Sanitize HTML to the Telegram-supported subset and ensure well-formed tags.
/// Allowed tags: b,strong,i,em,u,ins,s,strike,del,code,pre,a,span(class=tg-spoiler),blockquote
/// Allowed attributes: a[href] with http/https/tg schemes; span[class=tg-spoiler]; code[class=language-xxx] (optional)
pub fn sanitize_telegram_html(input: &str) -> String {
    // Normalize alternative spoiler tag to span.tg-spoiler
    let normalized = input
        .replace("<tg-spoiler>", r#"<span class=\"tg-spoiler\">"#)
        .replace("</tg-spoiler>", "</span>");

    // Build sanitizer
    let allowed_tags = [
        "b", "strong", "i", "em", "u", "ins", "s", "strike", "del", "code", "pre", "a", "span",
        // Keep blockquote allowed as internal templates use it (e.g., sentinel notifications)
        "blockquote",
    ];

    let mut builder = HtmlSanitizerBuilder::default();
    // Set exact allowed tags
    builder.tags(allowed_tags.iter().cloned().collect());
    // Whitelist attributes per tag via a map
    let mut tag_attrs: std::collections::HashMap<&str, std::collections::HashSet<&str>> =
        std::collections::HashMap::new();
    tag_attrs.insert("a", std::iter::once("href").collect());
    tag_attrs.insert("span", std::iter::once("class").collect());
    tag_attrs.insert("code", std::iter::once("class").collect());
    builder.tag_attributes(tag_attrs);
    // Restrict URL schemes
    builder.url_schemes(["http", "https", "tg"].iter().cloned().collect());
    // Drop all generic attributes
    builder.generic_attributes(std::collections::HashSet::new());

    let mut cleaned = builder.clean(&normalized).to_string();

    // Post-filter: remove span class if not tg-spoiler, and code class if it doesn't start with "language-"
    let re_span_class = Regex::new(r#"(<span[^>]*?)\sclass=\"([^\"]*)\"([^>]*>)"#).unwrap();
    cleaned = re_span_class
        .replace_all(&cleaned, |caps: &regex::Captures| {
            let class_val = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if class_val == "tg-spoiler" {
                caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string()
            } else {
                format!("{}{}", caps.get(1).unwrap().as_str(), caps.get(3).unwrap().as_str())
            }
        })
        .to_string();
    // This is a best-effort; sanitizer already ensures only safe attributes remain.
    let re_code_class = Regex::new(r#"(<code[^>]*?)\sclass=\"([^\"]*)\"([^>]*>)"#).unwrap();
    cleaned = re_code_class
        .replace_all(&cleaned, |caps: &regex::Captures| {
            let class_val = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if class_val.starts_with("language-") {
                caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string()
            } else {
                format!("{}{}", caps.get(1).unwrap().as_str(), caps.get(3).unwrap().as_str())
            }
        })
        .to_string();

    // Minor whitespace tidy (avoid trailing spaces that sometimes cause entity issues)
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::sanitize_telegram_html;

    #[test]
    fn strips_unsupported_tags() {
        let input = "<div>Hello <span class=\"tg-spoiler\">there</span></div>";
        let out = sanitize_telegram_html(input);
        assert!(out.contains("Hello"));
        // div should be gone; spoiler span remains
        assert!(!out.contains("<div>"));
        assert!(out.contains("<span class=\"tg-spoiler\">there</span>"));
    }

    #[test]
    fn normalizes_tg_spoiler_tag() {
        let input = "Revealed <tg-spoiler>secret</tg-spoiler> text";
        let out = sanitize_telegram_html(input);
        assert!(out.contains("<span class=\"tg-spoiler\">secret</span>"));
        assert!(!out.contains("tg-spoiler>secret</tg-spoiler"));
    }

    #[test]
    fn blocks_javascript_links() {
        let input = "<a href=\"javascript:alert(1)\">click</a>";
        let out = sanitize_telegram_html(input);
        // The href should be dropped; at minimum no javascript remains
        assert!(!out.to_lowercase().contains("javascript:"));
        assert!(out.contains("<a>click</a>") || out.contains(">click<"));
    }

    #[test]
    fn allows_http_and_tg_links() {
        let input = "<a href=\"https://example.com\">ok</a> and <a href=\"tg://user?id=123\">mention</a>";
        let out = sanitize_telegram_html(input);
        assert!(out.contains("href=\"https://example.com\""));
        assert!(out.contains("href=\"tg://user?id=123\""));
    }

    #[test]
    fn preserves_pre_and_code_and_language_class() {
        let input = "<pre><code class=\"language-rust\">fn main() {}</code></pre>";
        let out = sanitize_telegram_html(input);
        assert!(out.contains("<pre>"));
        assert!(out.contains("<code class=\"language-rust\">"));
    }

    #[test]
    fn strips_non_language_code_class() {
        let input = "<pre><code class=\"alert\">boom</code></pre>";
        let out = sanitize_telegram_html(input);
        // class attribute removed when not language-*
        assert!(out.contains("<pre>"));
        assert!(out.contains("<code>boom</code>"));
        assert!(!out.contains("class=\"alert\""));
    }

    #[test]
    fn allows_blockquote() {
        let input = "<blockquote>quote</blockquote>";
        let out = sanitize_telegram_html(input);
        assert!(out.contains("<blockquote>quote</blockquote>"));
    }
}

pub fn normalize_image_url_anchor(text: &str) -> String {
    let re_gcs = Regex::new(r#"https://storage\.googleapis\.com/[^\s<>\"]+"#).unwrap();
    let gcs = if let Some(m) = re_gcs.find(text) {
        m.as_str().to_string()
    } else {
        return text.to_string();
    };

    let re_anchor = Regex::new(r#"(?i)(Image URL:\s*)<a\s+href=\"[^\"]+\">([^<]*)</a>"#).unwrap();
    let replacement = format!(r#"$1<a href=\"{}\">$2</a>"#, gcs);
    re_anchor.replace(text, replacement.as_str()).to_string()
}

/// Unescape essential markdown characters for welcome messages and filters
pub fn unescape_markdown(text: &str) -> String {
    let mut result = text.to_string();

    // Unescape essential markdown characters for welcome messages and filters
    result = result.replace("\\*", "*"); // Bold/italic (very common)
    result = result.replace("\\_", "_"); // Underline (less common)
    result = result.replace("\\`", "`"); // Inline code (common for addresses, commands)
    result = result.replace("\\{", "{"); // Placeholders (essential)
    result = result.replace("\\}", "}"); // Placeholders (essential)

    result
}

/// Escape dynamic content for MarkdownV2 to prevent parsing errors
pub fn escape_for_markdown_v2(text: &str) -> String {
    let mut result = text.to_string();

    // Escape MarkdownV2 special characters in dynamic content
    result = result.replace("_", "\\_"); // Underline
    result = result.replace("*", "\\*"); // Bold/italic
    result = result.replace("[", "\\["); // Links
    result = result.replace("]", "\\]"); // Links
    result = result.replace("(", "\\("); // Links
    result = result.replace(")", "\\)"); // Links
    result = result.replace("~", "\\~"); // Strikethrough
    result = result.replace("`", "\\`"); // Inline code
    result = result.replace(">", "\\>"); // Blockquote
    result = result.replace("#", "\\#"); // Headers
    result = result.replace("+", "\\+"); // Lists
    result = result.replace("-", "\\-"); // Lists
    result = result.replace("=", "\\="); // Headers
    result = result.replace("|", "\\|"); // Tables
    result = result.replace("{", "\\{"); // Code blocks
    result = result.replace("}", "\\}"); // Code blocks
    result = result.replace(".", "\\."); // Periods (reserved)
    result = result.replace("!", "\\!"); // Exclamation marks (reserved)

    result
}

pub async fn create_purchase_request(
    file_search_calls: u32,
    web_search_calls: u32,
    image_generation_calls: u32,
    total_tokens_used: u32,
    model: Model,
    token: &str,
    mut group_id: Option<String>,
    user_id: Option<String>,
    bot_deps: BotDependencies,
) -> Result<(), anyhow::Error> {
    // Resolve currency/version from user or group prefs; fallback to on-chain default
    let (currency, coin_version) = if let Some(gid) = &group_id {
        let key = gid.clone();
        let prefs: Option<crate::payment::dto::PaymentPrefs> =
            bot_deps.payment.get_payment_token(key, &bot_deps).await;
        if prefs.is_some() {
            let prefs = prefs.unwrap();
            (prefs.currency, prefs.version)
        } else {
            (
                bot_deps.default_payment_prefs.currency,
                bot_deps.default_payment_prefs.version,
            )
        }
    } else {
        let key = user_id.unwrap();
        let prefs: Option<crate::payment::dto::PaymentPrefs> =
            bot_deps.payment.get_payment_token(key, &bot_deps).await;
        if prefs.is_some() {
            let prefs = prefs.unwrap();
            (prefs.currency, prefs.version)
        } else {
            (
                bot_deps.default_payment_prefs.currency,
                bot_deps.default_payment_prefs.version,
            )
        }
    };
    let mut tools_used = Vec::new();
    let account_seed =
        env::var("ACCOUNT_SEED").map_err(|e| anyhow::anyhow!("ACCOUNT_SEED is not set: {}", e))?;

    if file_search_calls > 0 {
        tools_used.push(ToolUsage {
            tool: AITool::FileSearch,
            calls: file_search_calls,
        });
    };
    if web_search_calls > 0 {
        tools_used.push(ToolUsage {
            tool: AITool::WebSearchPreview,
            calls: web_search_calls,
        });
    };
    if image_generation_calls > 0 {
        tools_used.push(ToolUsage {
            tool: AITool::ImageGeneration,
            calls: image_generation_calls,
        });
    };

    if group_id.is_some() {
        let group_id_result = group_id.unwrap();
        let group_id_with_seed = format!("{}-{}", group_id_result, account_seed);
        group_id = Some(group_id_with_seed);
    }

    let purchase_request = PurchaseRequest {
        model,
        currency,
        coin_version,
        tokens_used: total_tokens_used,
        tools_used,
        group_id: group_id.clone(),
    };

    let response = if group_id.is_some() {
        bot_deps
            .service
            .group_purchase(token.to_string(), purchase_request)
            .await
    } else {
        bot_deps
            .service
            .purchase(token.to_string(), purchase_request)
            .await
    };

    match response {
        Ok(_) => Ok(()),
        Err(e) => {
            log::error!("Error purchasing tokens: {}", e);
            Err(e)
        }
    }
}

pub async fn is_admin(bot: &Bot, chat_id: ChatId, user_id: UserId) -> bool {
    let admins = bot.get_chat_administrators(chat_id).await;

    if admins.is_err() {
        return false;
    }

    let admins = admins.unwrap();
    let is_admin = admins.iter().any(|member| member.user.id == user_id);
    is_admin
}

pub async fn send_message(msg: Message, bot: Bot, text: String) -> Result<(), anyhow::Error> {
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, text).reply_to(msg.id).await?;
    } else {
        bot.send_message(msg.chat.id, text).await?;
    }

    Ok(())
}

pub async fn send_html_message(msg: Message, bot: Bot, text: String) -> Result<(), anyhow::Error> {
    let text = sanitize_telegram_html(&text);
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_to(msg.id)
            .await?;
    } else {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .await?;
    }

    Ok(())
}

pub async fn send_markdown_message(
    msg: Message,
    bot: Bot,
    text: String,
) -> Result<(), anyhow::Error> {
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_to(msg.id)
            .await?;
    } else {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::MarkdownV2)
            .await?;
    }
    Ok(())
}

pub async fn send_scheduled_message(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
) -> Result<Message, RequestError> {
    // For scheduled messages, send to thread if thread_id is available
    let text = sanitize_telegram_html(text);
    let mut request = bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html);

    if let Some(thread) = thread_id {
        request = request.reply_to(MessageId(thread));
    }

    request.await
}

pub async fn send_markdown_message_with_keyboard(
    bot: Bot,
    msg: Message,
    keyboard_markup_type: KeyboardMarkupType,
    text: &str,
) -> Result<(), RequestError> {
    match keyboard_markup_type {
        KeyboardMarkupType::InlineKeyboardType(keyboard_markup) => {
            reply_inline_markup(bot, msg, keyboard_markup, text).await?
        }
        KeyboardMarkupType::KeyboardType(keyboard_markup) => {
            reply_markup(bot, msg, keyboard_markup, text).await?
        }
    };
    Ok(())
}

pub async fn reply_markup(
    bot: Bot,
    msg: Message,
    keyboard_markup: KeyboardMarkup,
    text: &str,
) -> Result<(), RequestError> {
    let text = sanitize_telegram_html(text);
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard_markup)
            .reply_to(msg.id)
            .await?;
    } else {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard_markup)
            .await?;
    }
    Ok(())
}

pub async fn reply_inline_markup(
    bot: Bot,
    msg: Message,
    keyboard_markup: InlineKeyboardMarkup,
    text: &str,
) -> Result<(), RequestError> {
    let text = sanitize_telegram_html(text);
    if msg.chat.is_group() || msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard_markup)
            .reply_to(msg.id)
            .await?;
    } else {
        bot.send_message(msg.chat.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard_markup)
            .await?;
    }
    Ok(())
}

pub async fn send_markdown_message_with_keyboard_with_reply(
    bot: Bot,
    msg: Message,
    keyboard_markup: KeyboardMarkupType,
    text: &str,
) -> Result<Message, RequestError> {
    let mut request = bot.send_message(msg.chat.id, text);

    match keyboard_markup {
        KeyboardMarkupType::InlineKeyboardType(inline_keyboard) => {
            request = request.reply_markup(inline_keyboard);
        }
        KeyboardMarkupType::KeyboardType(keyboard) => {
            request = request.reply_markup(keyboard);
        }
    }

    if msg.chat.is_group() || msg.chat.is_supergroup() {
        request = request.reply_to(msg.id);
    }

    request.await.map_err(|e| e.into())
}

pub async fn send_scheduled_message_with_keyboard(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
    keyboard: InlineKeyboardMarkup,
) -> Result<Message, RequestError> {
    let mut request = bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html)
        .reply_markup(keyboard);

    if let Some(thread) = thread_id {
        request = request.reply_to(MessageId(thread));
    }

    request.await
}
