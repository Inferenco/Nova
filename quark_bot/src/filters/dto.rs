use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilterDefinition {
    pub trigger: String,
    pub response: String,
    pub group_id: String,
    pub created_by: i64,
    pub created_at: i64,
    pub is_active: bool,
    pub match_type: MatchType,
    pub response_type: ResponseType,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum MatchType {
    Exact,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ResponseType {
    Text,
    Markdown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilterMetadata {
    pub group_id: String,
    pub trigger_hash: String,
    pub display_name: String,
    pub response_preview: String,
    pub last_modified: i64,
    pub modified_by: i64,
    pub filter_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilterStats {
    pub group_id: String,
    pub filter_id: String,
    pub usage_count: u64,
    pub last_triggered: Option<i64>,
    pub last_triggered_by: Option<i64>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FilterMatch {
    pub filter: FilterDefinition,
    pub matched_text: String,
    pub match_position: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateFilterRequest {
    pub trigger: String,
    pub response: String,
    pub match_type: MatchType,
    pub response_type: ResponseType,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FilterListResponse {
    pub filters: Vec<FilterDefinition>,
    pub total_count: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PendingFilterStep {
    AwaitingTrigger,
    AwaitingResponse,
    AwaitingConfirm,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingFilterWizardState {
    pub group_id: i64,
    pub creator_user_id: i64,
    pub step: PendingFilterStep,
    pub trigger: Option<String>,
    pub response: Option<String>,
    pub match_type: MatchType,
    pub response_type: ResponseType,
    #[serde(default)]
    pub current_bot_message_id: Option<i32>,
    #[serde(default)]
    pub user_message_ids: Vec<i32>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum FilterError {
    NotFound(String),
    ValidationFailed(ValidationResult),
    DatabaseError(String),
    PermissionDenied(String),
    DuplicateFilter(String),
    InternalError(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::NotFound(msg) => write!(f, "Filter not found: {}", msg),
            FilterError::ValidationFailed(result) => {
                write!(f, "Validation failed: {}", result.errors.join(", "))
            }
            FilterError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
            FilterError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            FilterError::DuplicateFilter(msg) => write!(f, "Duplicate filter: {}", msg),
            FilterError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for FilterError {}

impl From<(String, String, String, i64, MatchType, ResponseType)> for FilterDefinition {
    fn from(value: (String, String, String, i64, MatchType, ResponseType)) -> Self {
        let (trigger, response, group_id, created_by, match_type, response_type) = value;
        let now = chrono::Utc::now().timestamp();
        let id = uuid::Uuid::new_v4().to_string();

        FilterDefinition {
            trigger,
            response,
            group_id,
            created_by,
            created_at: now,
            is_active: true,
            match_type,
            response_type,
            id,
        }
    }
}

impl ValidationResult {
    pub fn success() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn failure(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: Vec::new(),
        }
    }
}
