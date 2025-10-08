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
    types::{
        ChatId, InlineKeyboardMarkup, KeyboardMarkup, LinkPreviewOptions, MessageId, ParseMode,
        UserId,
    },
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

/// NOTE: Sanitization for Telegram HTML is handled by `sanitize_ai_html` and is
/// only applied to AI-generated content. Predefined/static messages should be
/// authored as valid Telegram HTML directly without additional sanitization.

/// Sanitize AI-generated HTML to the Telegram-supported subset and ensure well-formed tags.
/// Allowed tags: b,strong,i,em,u,ins,s,strike,del,code,pre,a,span(class=tg-spoiler),blockquote
/// Allowed attributes: a[href] with http/https/tg schemes; span[class=tg-spoiler]; code[class=language-xxx] (optional)
pub fn sanitize_ai_html(input: &str) -> String {
    // Pre-repair: handle stray `<tg-spoiler` without `>` to avoid empty output after parsing.
    // Strategy: protect proper `<tg-spoiler>` with a placeholder, then add a missing `>` to any remaining `<tg-spoiler`.
    let placeholder = "__TG_SPOILER_OK__";
    let mut pre = input.replace("<tg-spoiler>", placeholder);
    pre = pre.replace("<tg-spoiler", "<tg-spoiler>");
    let pre = pre.replace(placeholder, "<tg-spoiler>");

    // Normalize alternative spoiler tag to span.tg-spoiler
    let normalized = pre
        .replace("<tg-spoiler>", r#"<span class="tg-spoiler">"#)
        .replace("</tg-spoiler>", "</span>");

    // Optionally promote escaped allowed tags back to HTML outside <pre> blocks
    let aggressive = std::env::var("RENDER_INTENT")
        .map(|v| {
            let v = v.to_lowercase();
            v == "aggressive" || v == "on" || v == "true" || v == "1"
        })
        .unwrap_or(false);
    let mut promotable = normalized.clone();
    if aggressive {
        // Temporarily extract <pre> blocks
        let re_pre = Regex::new(r"(?is)<pre[^>]*>.*?</pre>").unwrap();
        let mut pre_blocks: Vec<String> = Vec::new();
        let mut i = 0usize;
        promotable = re_pre
            .replace_all(&promotable, |caps: &regex::Captures| {
                let s = caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
                pre_blocks.push(s);
                let ph = format!("__PRE_BLOCK_{}__", i);
                i += 1;
                ph
            })
            .to_string();

        // Promote escaped allowed tags e.g. &lt;b&gt; → <b>
        let re_escaped_allowed = Regex::new(
            r#"&lt;(/?)(b|strong|i|em|u|ins|s|strike|del|code|pre|a|span|blockquote)([^&]*)&gt;"#,
        )
        .unwrap();
        promotable = re_escaped_allowed
            .replace_all(&promotable, |caps: &regex::Captures| {
                let slash = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let tag = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let attrs = caps.get(3).map(|m| m.as_str()).unwrap_or("");
                format!("<{}{}{}>", slash, tag, attrs)
            })
            .to_string();

        // Greedy repair: ensure unclosed allowed tags are closed at the end of the fragment
        let tag_re = Regex::new(
            r"(?is)<\s*(/)?\s*(b|strong|i|em|u|ins|s|strike|del|code|pre|a|span|blockquote)\b[^>]*>",
        )
        .unwrap();
        let mut out = String::new();
        let mut last = 0usize;
        let mut stack: Vec<String> = Vec::new();
        for caps in tag_re.captures_iter(&promotable) {
            let m = caps.get(0).unwrap();
            let end = m.end();
            out.push_str(&promotable[last..end]);
            last = end;
            let is_end = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim().len() > 0;
            let name = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_lowercase();
            if !is_end {
                stack.push(name);
            } else {
                if let Some(pos) = stack.iter().rposition(|t| t == &name) {
                    stack.truncate(pos);
                }
            }
        }
        out.push_str(&promotable[last..]);
        if !stack.is_empty() {
            for name in stack.iter().rev() {
                out.push_str(&format!("</{}>", name));
            }
        }
        promotable = out;

        // Reinsert <pre> blocks
        for (idx, block) in pre_blocks.iter().enumerate() {
            let ph = format!("__PRE_BLOCK_{}__", idx);
            promotable = promotable.replace(&ph, block);
        }
    }

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

    let source = if aggressive { &promotable } else { &normalized };
    let mut cleaned = builder.clean(source).to_string();

    // Post-filter: remove span class if not tg-spoiler, and then unwrap any <span>...</span>
    // that doesn't carry class="tg-spoiler" (Telegram requires that class on span)
    let re_span_class = Regex::new(r#"(<span[^>]*?)\sclass=\"([^\"]*)\"([^>]*>)"#).unwrap();
    cleaned = re_span_class
        .replace_all(&cleaned, |caps: &regex::Captures| {
            let class_val = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if class_val == "tg-spoiler" {
                caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string()
            } else {
                // Drop non-spoiler classes entirely
                format!("{}{}", caps.get(1).unwrap().as_str(), caps.get(3).unwrap().as_str())
            }
        })
        .to_string();
    // Unwrap any remaining <span ...>...</span> that does not include class="tg-spoiler"
    let re_non_tg_span_pair = Regex::new(r"(?is)<span([^>]*)>(.*?)</span>").unwrap();
    cleaned = re_non_tg_span_pair
        .replace_all(&cleaned, |caps: &regex::Captures| {
            let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if attrs.contains("class=\"tg-spoiler\"") {
                // Keep spoiler span as-is
                caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string()
            } else {
                // Unwrap other spans (keep inner text only)
                caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string()
            }
        })
        .to_string();

    // Unwrap <a> tags that lack href after sanitization to avoid Telegram errors
    // (Rust regex doesn't support lookarounds; match anchors and check attrs in code.)
    let re_anchor = Regex::new(r"(?is)<a([^>]*)>(.*?)</a>").unwrap();
    cleaned = re_anchor
        .replace_all(&cleaned, |caps: &regex::Captures| {
            let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_lowercase();
            let inner = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if attrs.contains("href=") {
                // keep original anchor
                caps.get(0).map(|m| m.as_str()).unwrap_or("").to_string()
            } else {
                // unwrap to plain text
                inner.to_string()
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
    use super::{sanitize_ai_html, unescape_markdown};

    #[test]
    fn strips_unsupported_tags() {
        let input = "<div>Hello <span class=\"tg-spoiler\">there</span></div>";
        let out = sanitize_ai_html(input);
        assert!(out.contains("Hello"));
        // div should be gone; spoiler span remains
        assert!(!out.contains("<div>"));
        assert!(out.contains("<span class=\"tg-spoiler\">there</span>"));
    }

    #[test]
    fn normalizes_tg_spoiler_tag() {
        let input = "Revealed <tg-spoiler>secret</tg-spoiler> text";
        let out = sanitize_ai_html(input);
        assert!(out.contains("<span class=\"tg-spoiler\">secret</span>"));
        assert!(!out.contains("tg-spoiler>secret</tg-spoiler"));
    }

    #[test]
    fn blocks_javascript_links() {
        let input = "<a href=\"javascript:alert(1)\">click</a>";
        let out = sanitize_ai_html(input);
        // The href should be dropped and link text preserved
        assert!(!out.to_lowercase().contains("javascript:"));
        assert!(out.contains("click"));
    }

    #[test]
    fn allows_http_and_tg_links() {
        let input = "<a href=\"https://example.com\">ok</a> and <a href=\"tg://user?id=123\">mention</a>";
        let out = sanitize_ai_html(input);
        assert!(out.contains("href=\"https://example.com\""));
        assert!(out.contains("href=\"tg://user?id=123\""));
    }

    #[test]
    fn preserves_pre_and_code_and_language_class() {
        let input = "<pre><code class=\"language-rust\">fn main() {}</code></pre>";
        let out = sanitize_ai_html(input);
        assert!(out.contains("<pre>"));
        assert!(out.contains("<code class=\"language-rust\">"));
    }

    #[test]
    fn strips_non_language_code_class() {
        let input = "<pre><code class=\"alert\">boom</code></pre>";
        let out = sanitize_ai_html(input);
        // class attribute removed when not language-*
        assert!(out.contains("<pre>"));
        assert!(out.contains("<code>boom</code>"));
        assert!(!out.contains("class=\"alert\""));
    }

    #[test]
    fn allows_blockquote() {
        let input = "<blockquote>quote</blockquote>";
        let out = sanitize_ai_html(input);
        assert!(out.contains("<blockquote>quote</blockquote>"));
    }

    #[test]
    fn unescape_markdown_unescapes_standard_chars_but_keeps_literal_bang_and_dot() {
        let input = r"\_\*\[\]\(\)\~\`\>\#\+\-\=\|\{\}\.\!";
        let mut expected = String::from("_*[]()~`>#+-=|{}");
        expected.push('\\');
        expected.push('.');
        expected.push('\\');
        expected.push('!');

        assert_eq!(unescape_markdown(input), expected);
    }

    #[test]
    fn unescape_markdown_allows_image_syntax() {
        let input = r"\![alt](https://example.com)";
        assert_eq!(unescape_markdown(input), "![alt](https://example.com)");
    }

    #[test]
    fn unescape_markdown_preserves_unknown_escapes() {
        let input = r"\%keep\!";
        assert_eq!(unescape_markdown(input), r"\%keep\!");
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

// Full set of MarkdownV2 characters that Telegram escapes
const MARKDOWN_V2_ESCAPABLE_CHARS: [char; 18] = [
    '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
];

/// Unescape essential markdown characters for welcome messages and filters
pub fn unescape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) if MARKDOWN_V2_ESCAPABLE_CHARS.contains(&next) => {
                    match next {
                        '!' => {
                            // Preserve escape for literal exclamation marks unless part of image syntax
                            let mut peek_iter = chars.clone();
                            if matches!(peek_iter.next(), Some('[')) {
                                result.push('!');
                            } else {
                                result.push('\\');
                                result.push('!');
                            }
                        }
                        '.' => {
                            result.push('\\');
                            result.push('.');
                        }
                        _ => result.push(next),
                    }
                }
                Some(next) => {
                    result.push('\\');
                    result.push(next);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }

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

/// Ensure commonly problematic MarkdownV2 reserved characters are escaped prior to sending.
///
/// This focuses on characters that Telegram frequently reports as errors when left bare in
/// prose (`.`, `!`, `-`, and `>`), while preserving existing escapes, code blocks, and image
/// syntax.
pub fn ensure_markdown_v2_reserved_chars(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    let mut pending_escape = false;
    let mut active_code_fence: Option<usize> = None;
    let mut at_line_start = true;

    while let Some(ch) = chars.next() {
        if pending_escape {
            result.push(ch);
            at_line_start = ch == '\n';
            pending_escape = false;
            continue;
        }

        if ch == '\\' {
            result.push('\\');
            pending_escape = true;
            at_line_start = false;
            continue;
        }

        if ch == '`' {
            // Count how many backticks appear consecutively to handle fenced blocks.
            let mut run = 1usize;
            while let Some('`') = chars.peek() {
                chars.next();
                run += 1;
            }

            match active_code_fence {
                Some(current) if current == run => active_code_fence = None,
                None => active_code_fence = Some(run),
                Some(_) => {
                    // Nested or mismatched fences – retain existing state but still copy ticks.
                }
            }

            for _ in 0..run {
                result.push('`');
            }
            at_line_start = false;
            continue;
        }

        if ch == '\n' {
            result.push('\n');
            at_line_start = true;
            continue;
        }

        let in_code = active_code_fence.is_some();

        if !in_code && ch == '!' {
            if matches!(chars.peek(), Some('[')) {
                // Preserve Markdown image/link syntax `![...](...)`.
                result.push('!');
            } else {
                result.push('\\');
                result.push('!');
            }
            at_line_start = false;
            continue;
        }

        if !in_code && ch == '-' {
            let next_is_ws = chars
                .peek()
                .copied()
                .map(|c| c == ' ' || (c as u32) == 0x09)
                .unwrap_or(false);
            if at_line_start && next_is_ws {
                result.push('-');
            } else {
                result.push('\\');
                result.push('-');
            }
            at_line_start = false;
            continue;
        }

        if !in_code && ch == '>' {
            let next_is_ws = chars
                .peek()
                .copied()
                .map(|c| c == ' ' || (c as u32) == 0x09)
                .unwrap_or(false);
            if at_line_start && next_is_ws {
                result.push('>');
            } else {
                result.push('\\');
                result.push('>');
            }
            at_line_start = false;
            continue;
        }

        if !in_code && ch == '.' {
            result.push('\\');
            result.push(ch);
            at_line_start = false;
            continue;
        }

        result.push(ch);
        at_line_start = ch == '\n';
    }

    if pending_escape {
        result.push('\\');
    }

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

async fn send_scheduled_message_inner(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
    disable_preview: bool,
) -> Result<Message, RequestError> {
    // For scheduled messages, send to thread if thread_id is available
    let mut request = bot
        .send_message(chat_id, text)
        .parse_mode(ParseMode::Html);

    if let Some(thread) = thread_id {
        request = request.reply_to(MessageId(thread));
    }

    if disable_preview {
        let options = LinkPreviewOptions {
            is_disabled: true,
            url: None,
            prefer_small_media: false,
            prefer_large_media: false,
            show_above_text: false,
        };
        request = request.link_preview_options(options);
    }

    request.await
}

pub async fn send_scheduled_message(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
) -> Result<Message, RequestError> {
    send_scheduled_message_inner(bot, chat_id, text, thread_id, false).await
}

pub async fn send_scheduled_message_no_preview(
    bot: &Bot,
    chat_id: ChatId,
    text: &str,
    thread_id: Option<i32>,
) -> Result<Message, RequestError> {
    send_scheduled_message_inner(bot, chat_id, text, thread_id, true).await
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
