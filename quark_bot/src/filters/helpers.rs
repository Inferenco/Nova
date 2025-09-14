// Parse trigger input supporting comma-separated tokens and bracketed multi-word tokens.
// Examples:
//   "[the contract], ca, contract" -> ["the contract", "ca", "contract"]
//   "hello, world" -> ["hello", "world"]
//   "[multi word] , single" -> ["multi word", "single"]
use crate::filters::dto::{PendingFilterWizardState, MatchType, ResponseType};
use crate::utils::{unescape_markdown, escape_for_markdown_v2};

pub fn parse_triggers(input: &str) -> Vec<String> {
    let mut triggers: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut in_brackets = false;

    for ch in input.chars() {
        match ch {
            '[' => {
                if in_brackets {
                    // Nested '[' treated as literal
                    buf.push(ch);
                } else {
                    in_brackets = true;
                }
            }
            ']' => {
                if in_brackets {
                    in_brackets = false;
                } else {
                    // Unmatched ']' treated as literal
                    buf.push(ch);
                }
            }
            ',' => {
                if in_brackets {
                    buf.push(ch);
                } else {
                    let token = buf.trim();
                    if !token.is_empty() {
                        triggers.push(strip_brackets(token).to_string());
                    }
                    buf.clear();
                }
            }
            _ => buf.push(ch),
        }
    }

    let token = buf.trim();
    if !token.is_empty() {
        triggers.push(strip_brackets(token).to_string());
    }

    // Normalize: trim and convert to lowercase for consistent storage and matching
    triggers
        .into_iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

fn strip_brackets(s: &str) -> &str {
    let s = s.trim();
    if s.starts_with('[') && s.ends_with(']') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

pub fn summarize(state: &PendingFilterWizardState) -> String {
    let trigger_input = state
        .trigger
        .as_deref()
        .unwrap_or("(trigger not set)");
    let triggers_display = if trigger_input == "(trigger not set)" {
        trigger_input.to_string()
    } else {
        let parts = parse_triggers(trigger_input);
        if parts.is_empty() {
            "(no valid triggers)".to_string()
        } else {
            parts
                .into_iter()
                .map(|t| format!("<code>{}</code>", t))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let response = state
        .response
        .as_deref()
        .unwrap_or("(response not set)");
    let match_type = match state.match_type {
        MatchType::Exact => "Exact word match",
        MatchType::Contains => "Contains anywhere",
        MatchType::StartsWith => "Message starts with",
        MatchType::EndsWith => "Message ends with",
    };
    format!(
        "🔍 <b>Filter Summary</b>\n\n📝 Triggers: {}\n💬 Response: <code>{}</code>\n🎯 Match type: {}\n📄 Format: MarkdownV2 (or plain text)",
        triggers_display, response, match_type
    )
}

/// Replace filter response placeholders with actual values
/// 
/// Available placeholders:
/// - {username} -> @username (with @ prefix for Telegram mentions)
/// - {group_name} -> actual group name
/// - {trigger} -> actual trigger word/phrase
pub fn replace_filter_placeholders(
    response: &str, 
    username: Option<&str>, 
    group_name: &str, 
    trigger: &str,
    response_type: ResponseType
) -> String {
    let mut result = response.to_string();
    
    match response_type {
        ResponseType::Markdown => {
            // For markdown responses, unescape and then escape for MarkdownV2
            result = unescape_markdown(&result);
            
            // Replace username with @ prefix for Telegram mentions, then escape for MarkdownV2
            let username_display = if let Some(username) = username {
                escape_for_markdown_v2(&format!("@{}", username))
            } else {
                escape_for_markdown_v2("User")
            };
            
            // Escape dynamic content for MarkdownV2 before replacement
            let escaped_group_name = escape_for_markdown_v2(group_name);
            let escaped_trigger = escape_for_markdown_v2(trigger);
            
            // Replace placeholders
            result = result.replace("{username}", &username_display);
            result = result.replace("{group_name}", &escaped_group_name);
            result = result.replace("{trigger}", &escaped_trigger);
        },
        ResponseType::Text => {
            // For text responses, just do simple placeholder replacement without any escaping
            let username_display = if let Some(username) = username {
                format!("@{}", username)
            } else {
                "User".to_string()
            };
            
            // Simple placeholder replacement for text - no escaping needed
            result = result.replace("{username}", &username_display);
            result = result.replace("{group_name}", group_name);
            result = result.replace("{trigger}", trigger);
        }
    }
    
    result
}


