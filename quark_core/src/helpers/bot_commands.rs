use teloxide::utils::command::BotCommands;

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
pub enum Command {
    #[command(description = "Open the Aptos Connect app.")]
    AptosConnect,
    #[command(description = "Log in as a user (DM only).", parse_with = "split")]
    LoginUser,
    #[command(description = "Login as a group admin.", parse_with = "split")]
    LoginGroup,
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "Start a new conversation thread.")]
    NewChat,
    #[command(description = "Send a prompt to the bot.")]
    C(String),
    #[command(description = "Send a prompt to the bot in a group.")]
    G(String),
    #[command(description = "Show example prompts.")]
    PromptExamples,
    #[command(description = "Open user settings menu (DM only).")]
    Usersettings,
    // Sentinel control moved into Group Settings → Moderation
    #[command(
        description = "Moderate content (reply to message) and send a report to the admin if content is found to be inappropriate, muting the user in this case."
    )]
    Report,
    #[command(description = "Show core and custom rules for this group.")]
    Rules,
    #[command(description = "Get your wallet address.")]
    WalletAddress,
    #[command(description = "Get your balance of a token.")]
    Balance(String),
    #[command(description = "Get the group's wallet address.")]
    GroupWalletAddress,
    #[command(description = "Get the group's balance of a token.")]
    GroupBalance(String),
    #[command(description = "Display model pricing information.")]
    Prices,
    #[command(description = "Watch a short tutorial video.")]
    Tutorial,
    #[command(
        description = "Send a global announcement (authorized only).",
        rename = "globalannouncement"
    )]
    Announcement(String),
    #[command(description = "Schedule a recurring or one-shot group prompt (admins only).")]
    SchedulePrompt,
    #[command(description = "List active scheduled prompts (admins only).")]
    ListScheduled,
    #[command(description = "Schedule a token payment to a user (group admins only).")]
    SchedulePayment,
    #[command(description = "List your scheduled token payments (group admins only).")]
    ListScheduledPayments,
    #[command(description = "Open group settings menu (admins only).")]
    Groupsettings,
}

#[derive(Debug, Clone, Default)]
pub enum QuarkState {
    #[default]
    Chat,
}
