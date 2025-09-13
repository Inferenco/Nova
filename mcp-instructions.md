## MCP (Model Context Protocol)
Integrate external knowledge systems via the built-in MCP tool. Declare an MCP server as a tool; the model may call it during a response.

## Quick start
```rust
use open_ai_rust_responses_by_sshift::{Client, Request, Model, Tool};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::from_env()?;

    // Optional: headers for the MCP server (e.g., auth)
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer example-token".to_string());

    // Define an MCP server tool
    let mcp_tool = Tool::mcp(
        "knowledge-server",
        "https://api.example.com/v1",
        Some(headers),
    );

    // Include the MCP tool in your request
    let request = Request::builder()
        .model(Model::GPT4oMini)
        .input("Use the external knowledge server to answer succinctly.")
        .tools(vec![mcp_tool])
        .max_output_tokens(500)
        .build();

    let response = client.responses.create(request).await?;
    println!("Answer: {}", response.output_text());
    Ok(())
}
```

## Approval modes
MCP tools support an optional `require_approval` setting:

- **auto**: Default behavior; the platform decides when to prompt for approval.
- **always**: Always require explicit approval before MCP calls.
- **never**: Never require approval.

Use `mcp_with_approval(...)` to set it explicitly:

```rust
use open_ai_rust_responses_by_sshift::Tool;
use std::collections::HashMap;

let mut headers = HashMap::new();
headers.insert("Authorization".to_string(), "Bearer example-token".to_string());

let auto = Tool::mcp("auto-knowledge", "https://api.auto.example/v1", Some(headers.clone()));
let manual = Tool::mcp_with_approval(
    "manual-knowledge",
    "https://api.manual.example/v1",
    "always",
    Some(headers),
);

// Then pass one or both tools in `.tools([...])` as needed.
// The SDK serializes `server_label`, `server_url`, `headers`, and `require_approval` into the request.
```

## Notes
- **Server URL**: Ensure your `server_url` points to a compliant MCP server.
- **Headers**: Provide any required headers (e.g., `Authorization`) as needed.
- **Approvals**: The SDK does not enforce approval locally; `require_approval` is forwarded to the API/platform handling approvals.
- **Responses**: Results from MCP usage are surfaced through standard response items/messages via the Responses API.