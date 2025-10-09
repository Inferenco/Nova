use crate::ai::actions::{
    execute_fear_and_greed_index, execute_get_recent_messages_for_chat, execute_get_time,
    execute_new_pools, execute_search_pools, execute_trending_pools,
};
use crate::ai::dto::AIResponse;
use crate::ai::gcs::GcsImageUploader;
use crate::ai::prompt::get_prompt;
use crate::ai::tools::{
    execute_custom_tool, get_all_custom_tools, get_fear_and_greed_index_tool, get_new_pools_tool,
    get_recent_messages_tool, get_search_pools_tool, get_time_tool, get_trending_pools_tool,
};
use crate::dependencies::BotDependencies;
use crate::payment::dto::PaymentPrefs;
use crate::user_conversation::handler::UserConversations;
use base64::{Engine as _, engine::general_purpose};
use open_ai_rust_responses_by_sshift::types::{
    Include, InputItem, ReasoningParams, Response, ResponseItem, Tool, ToolChoice,
};
use open_ai_rust_responses_by_sshift::{
    Client as OAIClient, FunctionCallInfo, Model, ReasoningEffort, RecoveryPolicy, Request,
};
use serde_json;
use teloxide::Bot;
use teloxide::types::{Message, User};

#[derive(Clone)]
pub struct AI {
    openai_client: OAIClient,
    system_prompt: String,
    cloud: GcsImageUploader,
}

impl AI {
    pub fn new(openai_api_key: String, cloud: GcsImageUploader) -> Self {
        let system_prompt = get_prompt();

        // Use default recovery policy for API error handling
        // This provides automatic retry with 1 attempt for seamless experience
        let recovery_policy = RecoveryPolicy::default();
        let openai_client = OAIClient::new_with_recovery(&openai_api_key, recovery_policy)
            .expect("Failed to create OpenAI client with recovery policy");

        Self {
            openai_client,
            system_prompt,
            cloud,
        }
    }

    pub fn get_client(&self) -> &OAIClient {
        &self.openai_client
    }

    pub async fn upload_user_images(
        &self,
        image_paths: Vec<(String, String)>,
    ) -> Result<Vec<String>, anyhow::Error> {
        if image_paths.is_empty() {
            return Ok(vec![]);
        }

        let mut urls = Vec::new();

        for (path, extension) in image_paths {
            let bytes = tokio::fs::read(&path).await?;
            let base64_data = general_purpose::STANDARD.encode(&bytes);
            match self
                .cloud
                .upload_base64_image(&base64_data, &extension, "quark/user_uploads")
                .await
            {
                Ok(url) => urls.push(url),
                Err(e) => log::error!("Failed to upload a user image {}: {}", path, e),
            }
            let _ = tokio::fs::remove_file(&path).await;
        }

        Ok(urls)
    }

    pub async fn generate_response(
        &self,
        bot: Bot,
        msg: Message,
        input: &str,
        image_url_from_reply: Option<String>,
        user_uploaded_image_urls: Vec<String>,
        model: Model,
        max_tokens: u32,
        reasoning: Option<ReasoningParams>,
        bot_deps: BotDependencies,
        group_id: Option<String>,
    ) -> Result<AIResponse, anyhow::Error> {
        let user: Option<User> = msg.from.clone();

        if user.is_none() {
            return Err(anyhow::anyhow!("User not found"));
        }

        let user = user.unwrap();
        let user_id = user.id.0 as i64;

        log::info!(
            "AI generate_response called for user {} with input: '{}'",
            user_id,
            input
        );

        let (address, jwt) = if group_id.is_some() {
            let group_credentials = bot_deps.group.get_credentials(msg.chat.id);

            if group_credentials.is_none() {
                return Err(anyhow::anyhow!("Group credentials not found"));
            }

            let group_credentials = group_credentials.unwrap();

            (
                group_credentials.resource_account_address,
                group_credentials.jwt,
            )
        } else {
            let username = user.username.clone();

            if username.is_none() {
                return Err(anyhow::anyhow!("Username not found"));
            }

            let username = username.unwrap();

            let user_credentials = bot_deps.auth.get_credentials(&username);

            if user_credentials.is_none() {
                return Err(anyhow::anyhow!("User credentials not found"));
            }

            let user_credentials = user_credentials.unwrap();

            (
                user_credentials.resource_account_address,
                user_credentials.jwt,
            )
        };

        let default_payment_prefs = bot_deps.default_payment_prefs.clone();

        let coin = if group_id.is_some() {
            bot_deps
                .payment
                .get_payment_token(group_id.clone().unwrap(), &bot_deps)
                .await
                .unwrap_or(PaymentPrefs::from((
                    default_payment_prefs.label,
                    default_payment_prefs.currency,
                    default_payment_prefs.version,
                )))
        } else {
            bot_deps
                .payment
                .get_payment_token(user_id.to_string(), &bot_deps)
                .await
                .unwrap_or(PaymentPrefs::from((
                    default_payment_prefs.label,
                    default_payment_prefs.currency,
                    default_payment_prefs.version,
                )))
        };

        let user_balance = bot_deps
            .panora
            .aptos
            .get_account_balance(&address, &coin.currency)
            .await;

        if user_balance.is_err() {
            return Err(anyhow::anyhow!("User balance not found"));
        }

        let user_balance = user_balance.unwrap();

        let token = bot_deps.panora.get_token_by_symbol(&coin.label).await;

        if token.is_err() {
            return Err(anyhow::anyhow!("Token not found"));
        }

        let token = token.unwrap();

        let token_price = token.usd_price;

        if token_price.is_none() {
            return Err(anyhow::anyhow!("Token price not found"));
        }

        let token_price = token_price.unwrap();

        let token_price = token_price.parse::<f64>();

        if token_price.is_err() {
            return Err(anyhow::anyhow!("Token price not found"));
        }

        let token_price = token_price.unwrap();

        let token_decimals = token.decimals;

        let min_deposit = bot_deps.panora.min_deposit / token_price;

        let min_deposit = (min_deposit as f64 * 10_f64.powi(token_decimals as i32)) as u64;

        if user_balance < min_deposit as i64 {
            let min_deposit_formatted = format!(
                "{:.2}",
                min_deposit as f64 / 10_f64.powi(token_decimals as i32)
            );

            let user_balance_formatted = format!(
                "{:.2}",
                user_balance as f64 / 10_f64.powi(token_decimals as i32)
            );

            return Err(anyhow::anyhow!(format!(
                "User balance is less than the minimum deposit. Please fund your account transfering {} to <code>{}</code> address. Minimum deposit: {} {} (Your balance: {} {})",
                token.symbol,
                address,
                min_deposit_formatted,
                token.symbol,
                user_balance_formatted,
                token.symbol
            )));
        }

        let user_convos = UserConversations::new(&bot_deps.db)?;

        // Check if we need to clear the thread from a previous summarization
        // But do this AFTER we get the current response, so AI can see its previous response
        let user_id_str = user_id.to_string();
        let should_clear_thread = if let Ok(should_clear) = bot_deps
            .summarizer
            .check_and_clear_pending_thread(&user_id_str, group_id.clone())
        {
            if should_clear {
                log::info!(
                    "Thread will be cleared for user {} after this response (delayed from previous summarization)",
                    user_id
                );
            }
            should_clear
        } else {
            false
        };

        let previous_response_id = user_convos.get_response_id(user_id);
        let mut tool_called: Vec<FunctionCallInfo> = Vec::new();

        // Track token usage across all API calls
        let mut total_prompt_tokens = 0u32;
        let mut total_output_tokens = 0u32;
        let mut total_tokens_used = 0u32;

        // Strict separation: group vs user vector stores
        let vector_store_id = if group_id.is_some() {
            // For /g commands: ONLY use group vector store, NO fallback to user
            let group_id_str = group_id.as_ref().unwrap();
            bot_deps
                .group_docs
                .get_group_vector_store_id(group_id_str.clone())
        } else {
            // For /c commands: ONLY use user vector store
            user_convos.get_vector_store_id(user_id)
        };

        // Enhanced tools: built-in tools + custom function tools
        let mut tools = vec![];

        // Add image generation only for non-O-series models (O-series don't support it)
        if !matches!(
            model,
            Model::O3 | Model::O4Mini | Model::O1 | Model::O1Mini | Model::O1Preview
        ) {
            tools.push(Tool::image_generation());
        }

        // Add web search for all models
        tools.push(Tool::web_search_preview());

        if let Some(vs_id) = vector_store_id.clone() {
            if !vs_id.is_empty() {
                tools.push(Tool::file_search(vec![vs_id]));
            }
        }

        // Add custom function tools (get_balance, withdraw_funds, recent_messages, etc.)
        // Full set is available for /c and /g
        tools.extend(get_all_custom_tools());

        let user = if group_id.is_some() {
            format!("group-{}", group_id.clone().unwrap())
        } else {
            format!("user-{}-{}", user_id, msg.chat.id.to_string())
        };

        let system_prompt = format!("Entity {}: {}", user, self.system_prompt);

        // Inject conversation summary if it exists
        let final_system_prompt = if let Some(summary) = bot_deps
            .summarizer
            .get_summary_for_instructions(&user_id_str, group_id.clone())
        {
            format!("{}\n\nSummary so far:\n{}", system_prompt, summary)
        } else {
            system_prompt
        };

        let mut request_builder = Request::builder()
            .model(model.clone())
            .instructions(final_system_prompt)
            .tools(tools.clone())
            .tool_choice(ToolChoice::auto())
            .parallel_tool_calls(true) // Enable parallel execution for efficiency
            .max_output_tokens(max_tokens)
            .user(&user)
            .store(true);

        // Apply user preferences based on model family
        if let Some(username) = msg.from.as_ref().and_then(|u| u.username.clone()) {
            let prefs = bot_deps.user_model_prefs.get_preferences(&username);

            match model {
                Model::GPT5 | Model::GPT5Mini => {
                    // GPT-5: apply verbosity and reasoning from user preferences
                    let verbosity = prefs.verbosity.to_openai_verbosity();
                    request_builder = request_builder.verbosity(verbosity);

                    // Apply reasoning if enabled (always low effort)
                    if prefs.reasoning_enabled {
                        request_builder =
                            request_builder.reasoning_effort(ReasoningEffort::Minimal);
                    }
                }
                // O-series and others: reasoning passed via ReasoningParams below when provided
                _ => {}
            }
        }

        if let Some(reasoning_params) = reasoning.clone() {
            request_builder = request_builder.reasoning(reasoning_params);
        }

        // ---- Attach vision inputs using the SDK helper (0.2.1) ----
        // Collect all image URLs we want GPT-4o to see
        let mut image_urls: Vec<String> = Vec::new();
        if let Some(url) = image_url_from_reply {
            image_urls.push(url);
        }
        image_urls.extend(user_uploaded_image_urls.clone());

        if !image_urls.is_empty() {
            let mut content = Vec::new();
            // Add all images to the content block with detail level 'high'
            for url in image_urls {
                content.push(InputItem::content_image_with_detail(&url, "high"));
            }
            // Add the text prompt to the content block
            if !input.trim().is_empty() {
                content.push(InputItem::content_text(input));
            }
            // Manually construct the message with multiple content items
            request_builder =
                request_builder.input_items(vec![InputItem::message("user", content)]);
        } else {
            // No images ⇒ plain text input as before
            request_builder = request_builder.input(input);
            // With no vision payload we can safely include file-search results if user has a vector store
            if vector_store_id.is_some() {
                request_builder = request_builder.include(vec![Include::FileSearchResults]);
            }
        }

        if let Some(prev_id) = previous_response_id.clone() {
            request_builder = request_builder.previous_response_id(prev_id);
        }

        let request = request_builder.build();
        log::info!("Making OpenAI API call with {} tools", tools.len());
        for tool in &tools {
            if let Some(func) = &tool.function {
                log::info!("Tool available: {}", func.name);
            }
        }

        log::info!("About to call OpenAI API...");
        let mut current_response: Response = match self
            .openai_client
            .responses
            .create(request)
            .await
        {
            Ok(response) => {
                log::info!("OpenAI API call successful, response ID: {}", response.id());

                // Extract and accumulate token usage
                if let Some(usage) = &response.usage {
                    total_prompt_tokens += usage.input_tokens;
                    total_output_tokens += usage.output_tokens;
                    total_tokens_used += usage.total_tokens;
                    log::info!(
                        "Initial API call tokens: input={}, output={}, total={}",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.total_tokens
                    );
                }

                response
            }
            Err(e) => {
                let error_msg = e.to_string();
                log::error!("OpenAI API call failed: {}", error_msg);

                // Handle vector store not found errors
                if error_msg.contains("Vector store") && error_msg.contains("not found") {
                    if group_id.is_some() {
                        // Group vector store cleanup
                        let group_id_str = group_id.as_ref().unwrap();
                        log::warn!(
                            "Group vector store not found, clearing orphaned reference for group {}",
                            group_id_str
                        );
                        if let Err(clear_err) = bot_deps
                            .group_docs
                            .cleanup_orphaned_group_vector_store(group_id_str.clone())
                        {
                            log::error!(
                                "Failed to clean up orphaned group vector store: {}",
                                clear_err
                            );
                        }
                        return Err(anyhow::anyhow!(
                            "Your group document library is no longer available (vector store deleted). Please upload files again via Group Settings → Document Library → Upload Files to create a new document library."
                        ));
                    } else {
                        // User vector store cleanup
                        log::warn!(
                            "Vector store not found, clearing orphaned reference for user {}",
                            user_id
                        );
                        if let Err(clear_err) = user_convos.cleanup_orphaned_vector_store(user_id) {
                            log::error!("Failed to clean up orphaned vector store: {}", clear_err);
                        }
                        return Err(anyhow::anyhow!(
                            "Your document library is no longer available (vector store deleted). Please upload files again via /usersettings → Document Library → Upload Files to create a new document library."
                        ));
                    }
                }

                return Err(e.into());
            }
        };

        // Enhanced function calling loop (following demo script pattern)
        let mut iteration = 1;
        const MAX_ITERATIONS: usize = 5; // Prevent infinite loops

        log::info!(
            "Initial response has {} tool calls",
            current_response.tool_calls().len()
        );

        while !current_response.tool_calls().is_empty() && iteration <= MAX_ITERATIONS {
            let tool_calls = current_response.tool_calls();
            log::info!(
                "AI Response has {} tool calls in iteration {}",
                tool_calls.len(),
                iteration
            );

            // Log all tool calls first
            for tc in &tool_calls {
                log::info!("Tool call found: {} with call_id: {}", tc.name, tc.call_id);
            }

            // Filter for custom function calls (get_balance, get_wallet_address, withdraw_funds, fund_account, get_trending_pools, search_pools, get_current_time, get_fear_and_greed_index, get_pay_users, get_recent_messages)
            let custom_tool_calls: Vec<_> = tool_calls
                .iter()
                .filter(|tc| {
                    tc.name == "get_balance"
                        || tc.name == "get_wallet_address"
                        || tc.name == "withdraw_funds"
                        || tc.name == "fund_account"
                        || tc.name == "get_trending_pools"
                        || tc.name == "search_pools"
                        || tc.name == "get_new_pools"
                        || tc.name == "get_current_time"
                        || tc.name == "get_fear_and_greed_index"
                        || tc.name == "get_pay_users"
                        || tc.name == "create_proposal"
                        || tc.name == "get_recent_messages"
                })
                .collect();

            tool_called.extend(custom_tool_calls.iter().map(|tc| (*tc).clone()));

            log::info!(
                "Found {} custom tool calls out of {} total",
                custom_tool_calls.len(),
                tool_calls.len()
            );

            if !custom_tool_calls.is_empty() {
                // Handle parallel custom function calls
                let mut function_outputs = Vec::new();
                for tool_call in &custom_tool_calls {
                    log::info!(
                        "Executing custom tool: {} with call_id: {}",
                        tool_call.name,
                        tool_call.call_id
                    );

                    // Parse arguments as JSON Value for execute_custom_tool
                    let args_value: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                        .unwrap_or_else(|e| {
                            log::error!("Failed to parse tool arguments: {}", e);
                            serde_json::json!({})
                        });

                    let result = execute_custom_tool(
                        &tool_call.name,
                        &args_value,
                        bot.clone(),
                        msg.clone(),
                        group_id.clone(),
                        bot_deps.clone(),
                    )
                    .await;

                    log::info!(
                        "Tool {} executed successfully, result length: {}",
                        tool_call.name,
                        result.len()
                    );

                    // Ensure result is not empty
                    let final_result = if result.trim().is_empty() {
                        log::warn!(
                            "Tool {} returned empty result, providing default",
                            tool_call.name
                        );
                        format!("Tool '{}' completed but returned no output", tool_call.name)
                    } else {
                        result
                    };

                    function_outputs.push((tool_call.call_id.clone(), final_result));
                }

                // Submit tool outputs using Responses API pattern (with_function_outputs)
                let mut continuation_builder = Request::builder()
                    .model(model.clone())
                    .with_function_outputs(current_response.id(), function_outputs)
                    .tools(tools.clone()) // Keep tools available for follow-ups
                    .instructions(self.system_prompt.clone())
                    .parallel_tool_calls(true)
                    .max_output_tokens(max_tokens)
                    .user(&format!("user-{}", user_id))
                    .store(true);

                if let Some(reasoning_params) = reasoning.clone() {
                    continuation_builder = continuation_builder.reasoning(reasoning_params);
                }

                let continuation_request = continuation_builder.build();

                log::info!("Making continuation request to OpenAI");
                current_response = self
                    .openai_client
                    .responses
                    .create(continuation_request)
                    .await?;
                log::info!("Continuation request completed");

                // Extract and accumulate token usage from continuation
                if let Some(usage) = &current_response.usage {
                    total_prompt_tokens += usage.input_tokens;
                    total_output_tokens += usage.output_tokens;
                    total_tokens_used += usage.total_tokens;
                    log::info!(
                        "Continuation API call tokens: input={}, output={}, total={}",
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.total_tokens
                    );
                }
            } else {
                // No custom function calls, break the loop
                // (Built-in tools like web_search and file_search are handled automatically by OpenAI)
                break;
            }

            iteration += 1;
        }

        // Extract text and potentially image data from the final response
        let mut reply = current_response.output_text();
        
        // Fix literal \n escape sequences that sometimes appear in AI responses
        // This handles cases where tool outputs or AI-generated text contains escaped newlines
        reply = reply.replace("\\n", "\n");
        
        let response_id = current_response.id().to_string();

        // Save response ID for future conversation context
        user_convos.set_response_id(user_id, &response_id)?;
        log::info!(
            "Saved response ID {} for future conversation context",
            response_id
        );

        // Now clear the thread if it was pending from previous summarization
        // This ensures the AI had access to its previous response for this turn
        if should_clear_thread {
            log::info!(
                "Clearing conversation thread for user {} (delayed from previous summarization)",
                user_id
            );
            if let Err(e) = user_convos.clear_response_id(user_id) {
                log::error!("Failed to clear response_id for user {}: {}", user_id, e);
            }
        }

        // Check if summarization is needed and enabled using per-user preferences
        let effective_prefs = bot_deps
            .summarization_settings
            .get_effective_prefs(&user_id_str, group_id.clone());

        if effective_prefs.enabled {
            let token_limit = effective_prefs.token_limit;

            // Try to summarize, but don't fail the AI response if summarization fails
            match bot_deps
                .summarizer
                .check_and_summarize(
                    &user_id_str,
                    total_tokens_used,
                    token_limit,
                    input,
                    &reply,
                    bot_deps.clone(),
                    group_id.clone(),
                    &jwt,
                )
                .await
            {
                Ok(Some(_summary)) => {
                    if group_id.is_some() {
                        log::info!(
                            "Conversation summarized for group {}, thread will be cleared on next request",
                            group_id.clone().unwrap()
                        );
                    } else {
                        log::info!(
                            "Conversation summarized for user {}, thread will be cleared on next request",
                            user_id
                        );
                    }
                    // Don't clear response_id immediately - let the response stay visible
                    // The thread will be cleared on the next request
                }
                Ok(None) => {
                    // No summarization needed
                }
                Err(e) => {
                    log::error!(
                        "Summarization failed for user {}: {}, but continuing with AI response",
                        user_id,
                        e
                    );
                    // Continue with the response even if summarization fails
                }
            }
        }

        let mut image_data: Option<Vec<u8>> = None;
        for item in &current_response.output {
            if let ResponseItem::ImageGenerationCall { result, .. } = item {
                // Decode the base64 string to image bytes
                match general_purpose::STANDARD.decode(result) {
                    Ok(bytes) => {
                        image_data = Some(bytes);

                        // Upload to GCS and append URL to reply
                        match self
                            .cloud
                            .upload_base64_image(result, "png", "quark/images")
                            .await
                        {
                            Ok(url) => {
                                reply = format!(
                                    "{}\n\n<a href=\"{}\">Your image for download</a>",
                                    reply, url
                                );
                            }
                            Err(e) => log::error!("Failed to upload image to GCS: {}", e),
                        }

                        // We found our image, no need to look further
                        break;
                    }
                    Err(e) => {
                        // Log the error but don't fail the entire response
                        log::error!("Error decoding base64 image: {}", e);
                    }
                }
            }
        }

        log::info!(
            "Total conversation tokens: input={}, output={}, total={}",
            total_prompt_tokens,
            total_output_tokens,
            total_tokens_used
        );

        // Calculate tool usage from the final response
        let (web_search_count, file_search_count, image_generation_count, code_interpreter_count) =
            AIResponse::calculate_tool_usage(&current_response);

        log::info!(
            "Tool usage: web_search={}, file_search={}, image_generation={}, code_interpreter={}",
            web_search_count,
            file_search_count,
            image_generation_count,
            code_interpreter_count
        );

        Ok(AIResponse::from((
            reply,
            model,
            image_data,
            Some(tool_called),
            total_tokens_used,
            web_search_count,
            file_search_count,
            image_generation_count,
            code_interpreter_count,
        )))
    }

    /// Generate a response for a scheduled prompt in a group context, using a
    /// per-schedule conversation thread. Returns the AIResponse and the new
    /// response_id. Does not persist response_id in user_conversations.
    pub async fn generate_response_for_schedule(
        &self,
        input: &str,
        model: Model,
        max_tokens: u32,
        reasoning: Option<ReasoningParams>,
        bot_deps: BotDependencies,
        group_id: String,
        previous_response_id: Option<String>,
        schedule_id: &str,
        creator_user_id: i64,
        creator_username: String,
    ) -> Result<(AIResponse, String), anyhow::Error> {
        // Validate group credentials
        let group_chat_id_i64: i64 = group_id.parse().unwrap_or(0);
        let group_chat_id = teloxide::types::ChatId(group_chat_id_i64 as i64);
        let group_credentials = bot_deps
            .group
            .get_credentials(group_chat_id)
            .ok_or_else(|| anyhow::anyhow!("Group credentials not found"))?;

        // Token checks for group account
        let address = group_credentials.resource_account_address;

        let default_payment_prefs = bot_deps.default_payment_prefs.clone();
        let coin = bot_deps
            .payment
            .get_payment_token(group_id.clone(), &bot_deps)
            .await
            .unwrap_or(PaymentPrefs::from((
                default_payment_prefs.label,
                default_payment_prefs.currency,
                default_payment_prefs.version,
            )));

        let token = bot_deps.panora.get_token_by_symbol(&coin.label).await?;

        let token_price = token
            .usd_price
            .ok_or_else(|| anyhow::anyhow!("Token price not found"))?
            .parse::<f64>()?;
        let token_decimals = token.decimals;
        let min_deposit = (bot_deps.panora.min_deposit / token_price) as f64;
        let min_deposit = (min_deposit * 10_f64.powi(token_decimals as i32)) as u64;
        let group_balance = bot_deps
            .panora
            .aptos
            .get_account_balance(&address, &coin.currency)
            .await?;
        if group_balance < min_deposit as i64 {
            let min_deposit_formatted = format!(
                "{:.2}",
                min_deposit as f64 / 10_f64.powi(token_decimals as i32)
            );
            let group_balance_formatted = format!(
                "{:.2}",
                group_balance as f64 / 10_f64.powi(token_decimals as i32)
            );
            return Err(anyhow::anyhow!(format!(
                "User balance is less than the minimum deposit. Please fund your account transfering {} to <code>{}</code> address. Minimum deposit: {} {} (Your balance: {} {})",
                token.symbol.clone(),
                address,
                min_deposit_formatted,
                token.symbol.clone(),
                group_balance_formatted,
                token.symbol
            )));
        }

        // Use the group's vector store (if any)
        let group_docs = &bot_deps.group_docs;
        let vector_store_id = group_docs.get_group_vector_store_id(group_id.clone());

        // Tools setup
        let mut tools = vec![];
        if !matches!(
            model,
            Model::O3 | Model::O4Mini | Model::O1 | Model::O1Mini | Model::O1Preview
        ) {
            tools.push(Tool::image_generation());
        }
        tools.push(Tool::web_search_preview());
        if let Some(vs_id) = vector_store_id.clone() {
            if !vs_id.is_empty() {
                tools.push(Tool::file_search(vec![vs_id]));
            }
        }
        // For scheduled prompts, only expose the safe subset plus recent-messages
        tools.push(get_time_tool());
        tools.push(get_fear_and_greed_index_tool());
        tools.push(get_trending_pools_tool());
        tools.push(get_search_pools_tool());
        tools.push(get_new_pools_tool());
        tools.push(get_recent_messages_tool());

        // Label for per-schedule conversation identity (Responses API max length: 64)
        // Use a compact, deterministic label based only on schedule_id
        let sid_clean: String = schedule_id
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        let sid_short: String = sid_clean.chars().take(16).collect();
        let user_label = format!("schedule-{}", sid_short);
        let system_prompt = format!("Entity {}: {}", user_label, self.system_prompt);

        // Inject conversation summary if it exists (for scheduled prompts, use creator's summary)
        let creator_user_id_str = creator_user_id.to_string();
        let final_system_prompt = if let Some(summary) = bot_deps
            .summarizer
            .get_summary_for_instructions(&creator_user_id_str, Some(group_id.clone()))
        {
            format!("{}\n\nSummary so far:\n{}", system_prompt, summary)
        } else {
            system_prompt
        };

        let mut request_builder = Request::builder()
            .model(model.clone())
            .instructions(final_system_prompt)
            .tools(tools.clone())
            .tool_choice(ToolChoice::auto())
            .parallel_tool_calls(true)
            .max_output_tokens(max_tokens)
            .user(&user_label)
            .store(true)
            .input(input);

        // Apply user preferences based on model family
        match model {
            Model::GPT5 | Model::GPT5Mini => {
                let prefs = bot_deps.user_model_prefs.get_preferences(&creator_username);
                let verbosity = prefs.verbosity.to_openai_verbosity();
                request_builder = request_builder.verbosity(verbosity);

                // Apply reasoning if enabled (always low effort)
                if prefs.reasoning_enabled {
                    request_builder = request_builder.reasoning_effort(ReasoningEffort::Minimal);
                }
            }
            _ => {}
        }

        if let Some(reasoning_params) = reasoning.clone() {
            request_builder = request_builder.reasoning(reasoning_params);
        }
        if vector_store_id.is_some() {
            request_builder = request_builder.include(vec![Include::FileSearchResults]);
        }
        if let Some(prev_id) = previous_response_id.clone() {
            request_builder = request_builder.previous_response_id(prev_id);
        }

        log::info!(
            "[schedule] OpenAI call: user_label={}, model={:?}, prev_id_present={}, vector_store={}",
            user_label,
            model,
            previous_response_id.is_some(),
            vector_store_id.is_some()
        );

        // tools already include the safe subset + get_recent_messages

        let mut current_response: Response = self
            .openai_client
            .responses
            .create(request_builder.build())
            .await?;
        let mut total_tokens_used = 0u32;
        if let Some(usage) = &current_response.usage {
            total_tokens_used += usage.total_tokens;
        }
        // Handle safe custom tool calls or in-progress responses in a loop
        let mut iteration = 1usize;
        const MAX_ITERATIONS: usize = 8;
        while !current_response.tool_calls().is_empty() && iteration <= MAX_ITERATIONS {
            let tool_calls = current_response.tool_calls();
            let custom_tool_calls: Vec<_> = tool_calls
                .iter()
                .filter(|tc| {
                    tc.name == "get_current_time"
                        || tc.name == "get_fear_and_greed_index"
                        || tc.name == "get_trending_pools"
                        || tc.name == "search_pools"
                        || tc.name == "get_new_pools"
                        || tc.name == "get_recent_messages"
                })
                .collect();

            if !custom_tool_calls.is_empty() {
                log::info!(
                    "[schedule] executing {} custom tool calls (iteration {})",
                    custom_tool_calls.len(),
                    iteration
                );
                let mut function_outputs = Vec::new();
                for tc in &custom_tool_calls {
                    let args_value: serde_json::Value = serde_json::from_str(&tc.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let result = match tc.name.as_str() {
                        "get_current_time" => execute_get_time(&args_value).await,
                        "get_fear_and_greed_index" => {
                            execute_fear_and_greed_index(&args_value).await
                        }
                        "get_trending_pools" => execute_trending_pools(&args_value).await,
                        "search_pools" => execute_search_pools(&args_value).await,
                        "get_new_pools" => execute_new_pools(&args_value).await,
                        "get_recent_messages" => {
                            let chat_id_i64: i64 = group_id.parse().unwrap_or(0);
                            let chat_id = teloxide::types::ChatId(chat_id_i64 as i64);
                            execute_get_recent_messages_for_chat(chat_id, bot_deps.clone()).await
                        }
                        _ => String::new(),
                    };

                    let final_result = if result.trim().is_empty() {
                        log::warn!(
                            "[schedule] tool {} returned empty output; substituting fallback",
                            tc.name
                        );
                        format!("Tool '{}' completed but returned no output", tc.name)
                    } else {
                        result
                    };

                    function_outputs.push((tc.call_id.clone(), final_result));
                }

                let mut continuation_builder = Request::builder()
                    .model(model.clone())
                    .with_function_outputs(current_response.id(), function_outputs)
                    .tools(tools.clone())
                    .instructions(self.system_prompt.clone())
                    .parallel_tool_calls(true)
                    .max_output_tokens(max_tokens)
                    .user(&user_label)
                    .store(true);
                if let Some(reasoning_params) = reasoning.clone() {
                    continuation_builder = continuation_builder.reasoning(reasoning_params);
                }

                current_response = self
                    .openai_client
                    .responses
                    .create(continuation_builder.build())
                    .await?;
                if let Some(usage) = &current_response.usage {
                    total_tokens_used += usage.total_tokens;
                }
            } else {
                break;
            }

            iteration += 1;
        }

        if iteration > MAX_ITERATIONS {
            log::warn!(
                "[schedule] reached max iterations while waiting for response {}; proceeding with available output",
                current_response.id()
            );
        }

        let mut reply = current_response.output_text();
        
        // Fix literal \n escape sequences that sometimes appear in AI responses
        // This handles cases where tool outputs or AI-generated text contains escaped newlines
        reply = reply.replace("\\n", "\n");
        
        let new_response_id = current_response.id().to_string();

        let mut image_data: Option<Vec<u8>> = None;
        for item in &current_response.output {
            if let ResponseItem::ImageGenerationCall { result, .. } = item {
                match general_purpose::STANDARD.decode(result) {
                    Ok(bytes) => {
                        image_data = Some(bytes);
                        match self
                            .cloud
                            .upload_base64_image(result, "png", "quark/images")
                            .await
                        {
                            Ok(url) => {
                                reply = format!(
                                    "{}\n\n<a href=\"{}\">Your image for download</a>",
                                    reply, url
                                );
                            }
                            Err(e) => log::error!("Failed to upload image to GCS: {}", e),
                        }
                        break;
                    }
                    Err(e) => {
                        log::error!("Error decoding base64 image: {}", e);
                    }
                }
            }
        }

        let (web_search_count, file_search_count, image_generation_count, code_interpreter_count) =
            AIResponse::calculate_tool_usage(&current_response);

        let ai_resp = AIResponse::from((
            reply,
            model,
            image_data,
            None,
            total_tokens_used,
            web_search_count,
            file_search_count,
            image_generation_count,
            code_interpreter_count,
        ));

        Ok((ai_resp, new_response_id))
    }
}
