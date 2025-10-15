use std::fmt;
use reqwest::StatusCode;
use open_ai_rust_responses_by_sshift::types::{Response, ResponseItem};
use open_ai_rust_responses_by_sshift::{FunctionCallInfo, Model};

// GeckoTerminal API constants
pub const GECKO_MAX_RETRIES: usize = 3;
pub const GECKO_RETRY_BASE_DELAY_MS: u64 = 300;

/// Captures the different classes of failures that can occur when calling the GeckoTerminal API.
#[derive(Debug)]
pub enum GeckoRequestError {
    Network(reqwest::Error),
    ResponseRead(reqwest::Error),
    Http { status: StatusCode, body: String },
    Parse(serde_json::Error),
    Api(Vec<String>),
}

impl fmt::Display for GeckoRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(err) => write!(f, "network error: {}", err),
            Self::ResponseRead(err) => write!(f, "failed to read response body: {}", err),
            Self::Http { status, body } => write!(f, "HTTP {}: {}", status, body),
            Self::Parse(err) => write!(f, "failed to parse JSON payload: {}", err),
            Self::Api(messages) => {
                if messages.is_empty() {
                    write!(f, "GeckoTerminal returned an error response")
                } else if messages.len() == 1 {
                    write!(f, "{}", messages[0])
                } else {
                    write!(f, "{}", messages.join(" | "))
                }
            }
        }
    }
}

/// Expected structure of the `data` field inside a GeckoTerminal JSON payload.
pub enum GeckoPayloadShape {
    Collection,
    Object,
}

/// High-level classification describing whether the GeckoTerminal payload contained usable data.
pub enum GeckoPayloadState {
    Populated,
    Empty,
    Missing,
}

/// Represents the AI's response, which can include text and/or an image.
#[derive(Debug)]
pub struct AIResponse {
    pub text: String,
    pub model: Model,
    pub image_data: Option<Vec<u8>>,
    pub tool_calls: Option<Vec<FunctionCallInfo>>,
    pub total_tokens: u32,
    // Tool usage fields - optional for backward compatibility
    pub web_search: Option<u32>,
    pub file_search: Option<u32>,
    pub image_generation: Option<u32>,
    pub code_interpreter: Option<u32>,
}

impl AIResponse {
    /// Calculate tool usage from OpenAI Response by analyzing output array
    pub fn calculate_tool_usage(response: &Response) -> (u32, u32, u32, u32) {
        let mut web_search = 0u32;
        let mut file_search = 0u32;
        let mut image_generation = 0u32;
        let mut code_interpreter = 0u32;

        for item in &response.output {
            match item {
                ResponseItem::WebSearchCall { .. } => web_search += 1,
                ResponseItem::FileSearchCall { .. } => file_search += 1,
                ResponseItem::ImageGenerationCall { .. } => image_generation += 1,
                ResponseItem::CodeInterpreterCall { .. } => code_interpreter += 1,
                _ => {}
            }
        }

        (web_search, file_search, image_generation, code_interpreter)
    }

    /// Get raw counts for custom formatting
    pub fn get_tool_usage_counts(&self) -> (u32, u32, u32, u32) {
        (
            self.web_search.unwrap_or(0),
            self.file_search.unwrap_or(0),
            self.image_generation.unwrap_or(0),
            self.code_interpreter.unwrap_or(0),
        )
    }
}

impl
    From<(
        String,
        Model,
        Option<Vec<u8>>,
        Option<Vec<FunctionCallInfo>>,
        u32,
    )> for AIResponse
{
    fn from(
        value: (
            String,
            Model,
            Option<Vec<u8>>,
            Option<Vec<FunctionCallInfo>>,
            u32,
        ),
    ) -> Self {
        let (text, model, image_data, tool_calls, total_tokens) = value;

        Self {
            text,
            model,
            image_data,
            tool_calls,
            total_tokens,
            web_search: None,
            file_search: None,
            image_generation: None,
            code_interpreter: None,
        }
    }
}

// Enhanced constructor with tool usage tracking
impl
    From<(
        String,
        Model,
        Option<Vec<u8>>,
        Option<Vec<FunctionCallInfo>>,
        u32,
        u32,
        u32,
        u32,
        u32,
    )> for AIResponse
{
    fn from(
        value: (
            String,
            Model,
            Option<Vec<u8>>,
            Option<Vec<FunctionCallInfo>>,
            u32,
            u32,
            u32,
            u32,
            u32,
        ),
    ) -> Self {
        let (
            text,
            model,
            image_data,
            tool_calls,
            total_tokens,
            web_search,
            file_search,
            image_generation,
            code_interpreter,
        ) = value;

        Self {
            text,
            model,
            image_data,
            tool_calls,
            total_tokens,
            web_search: if web_search > 0 {
                Some(web_search)
            } else {
                None
            },
            file_search: if file_search > 0 {
                Some(file_search)
            } else {
                None
            },
            image_generation: if image_generation > 0 {
                Some(image_generation)
            } else {
                None
            },
            code_interpreter: if code_interpreter > 0 {
                Some(code_interpreter)
            } else {
                None
            },
        }
    }
}

// Backward compatibility constructor
impl
    From<(
        String,
        Model,
        Option<Vec<u8>>,
        Option<Vec<FunctionCallInfo>>,
    )> for AIResponse
{
    fn from(
        value: (
            String,
            Model,
            Option<Vec<u8>>,
            Option<Vec<FunctionCallInfo>>,
        ),
    ) -> Self {
        let (text, model, image_data, tool_calls) = value;

        Self {
            text,
            model,
            image_data,
            tool_calls,
            total_tokens: 0,
            web_search: None,
            file_search: None,
            image_generation: None,
            code_interpreter: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backward_compatibility() {
        // Test that existing usage tracking continues to work unchanged
        let ai_response =
            AIResponse::from(("Legacy response".to_string(), Model::GPT41Mini, None, None));

        assert_eq!(ai_response.total_tokens, 150);
        assert_eq!(ai_response.web_search, None);
        assert_eq!(ai_response.file_search, None);
        assert_eq!(ai_response.image_generation, None);
        assert_eq!(ai_response.code_interpreter, None);
    }
}
