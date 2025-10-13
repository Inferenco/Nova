use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CircleResponse<T> {
    #[schema(value_type = Object)]
    pub data: T,
}

impl<T> CircleResponse<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}
