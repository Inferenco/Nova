# Nova

Nova is a sophisticated AI-powered Telegram bot ecosystem with deep blockchain integration on Aptos. Built in Rust and TypeScript, Nova provides advanced group management, AI-driven conversations with tool-calling capabilities, automated payment systems, content moderation, and decentralized governance features. It's designed for communities that need intelligent automation, financial operations, and comprehensive administrative controls.

## 🏗️ Architecture

For detailed system architecture, component interactions, and data flow, see the [Architecture Documentation](Architecture.md).

## ✨ Key Features

### 🤖 AI & Automation
- **Advanced Conversational AI**: Context-aware conversations with multiple OpenAI models (GPT-5, GPT-5-mini)
- **AI Tool Calling**: Built-in tools for blockchain operations, market data, time queries, and web search
- **RAG-Enabled**: Separate vector stores for users and groups with document library management
- **Scheduled Automation**: Automated prompts and payments with flexible scheduling (5m to monthly intervals)

### 👥 Group Management
- **Content Moderation**: AI-powered content filtering with customizable rules and automatic enforcement
- **Custom Filters**: Create trigger-response automations with multiple match types and placeholders
- **Welcome System**: Dynamic member onboarding with personalized messages
- **Command Controls**: Granular command enable/disable per group
- **Migration Tools**: Group ID migration support for seamless transitions

### 🏛️ Governance & DAO
- **Proposal System**: Create and manage DAO proposals with detailed descriptions
- **Token-Weighted Voting**: Democratic decision-making based on token holdings
- **Automated Notifications**: Active proposal reminders and results announcements
- **Flexible Voting Periods**: Configurable start/end times for proposals

### 💰 Blockchain & Payments
- **Aptos Integration**: Native smart contract integration for users and groups
- **Multi-Token Support**: APT, USDC, USDT, and other Aptos tokens
- **Automated Payments**: Scheduled recurring payments to users or addresses
- **Group Wallets**: Shared group accounts with automatic resource management
- **Payment Tracking**: Balance queries, transaction history, and preferences

### 📊 Data & Analytics
- **Conversation Summarization**: Automatic summary generation for long discussions
- **Filter Statistics**: Usage tracking for custom triggers
- **DAO Analytics**: Proposal status tracking and voting results
- **Market Data**: Integration with GeckoTerminal and Panora for real-time crypto data

## 📋 Prerequisites

- **Rust** (latest stable) - [Install Rust](https://rustup.rs/)
- **Node.js** (v18+) - [Install Node.js](https://nodejs.org/)
- **Docker & Docker Compose** - [Install Docker](https://docs.docker.com/get-docker/)
- **Telegram Bot Token** - [Get from BotFather](https://core.telegram.org/bots#6-botfather)
- **OpenAI API Key** - [Get from OpenAI](https://platform.openai.com/account/api-keys)
- **Aptos Network Access** - Testnet or Mainnet configuration

## 🚀 Quick Start

### 1. Clone and Setup

```bash
git clone git@github.com:Inferenco/Nova.git
cd Nova
```

### 2. Environment Configuration

```bash
# Copy environment template
cp env.example .env

# Edit .env with your credentials
nano .env
```

**Required Environment Variables:**
- `TELOXIDE_TOKEN` - Your Telegram bot token
- `OPENAI_API_KEY` - Your OpenAI API key
- `APTOS_NETWORK` - Network (testnet/mainnet)
- `CONTRACT_ADDRESS` - Deployed contract address
- `VALKEY_PASSWORD` - Valkey/Redis password
- `PRIVATE_KEY` - Admin private key for blockchain operations

See [env.example](env.example) for all available options.

### 3. Run with Docker Compose (Recommended)

```bash
# Build and start all services
docker-compose up --build

# Run in background
docker-compose up -d --build

# View logs
docker-compose logs -f
```

## 🐳 Docker Compose Services

### Production Setup (`docker-compose.yml`)

```bash
# Start all services
docker-compose up -d

# Start specific services
docker-compose up quark-bot quark-server -d
docker-compose up quark-webhook -d
docker-compose up quark-consumer-one quark-consumer-two -d

# Stop all services
docker-compose down

# Stop and remove volumes
docker-compose down -v
```

**Available Services:**
- **quark-bot** - Main Telegram bot (port: internal)
- **quark-server** - REST API server (port: 3200)
- **quark-webhook** - Next.js web application (port: 3000)
- **quark-consumer-one** - Payment processor instance 1
- **quark-consumer-two** - Payment processor instance 2
- **valkey** - Redis-compatible message queue (port: 6379)

### Debug Setup (`docker-compose.debug.yml`)

```bash
# Start debug environment with LLDB support
docker-compose -f docker-compose.debug.yml up --build

# Run specific debug service
docker-compose -f docker-compose.debug.yml up quark-bot-lldb -d
```

**Debug Features:**
- LLDB remote debugging on port 1235
- Hot reload for source code changes
- Enhanced logging with backtraces
- Development environment variables

## 🛠️ Local Development

### Rust Components (Workspace)

```bash
# Build all components
cargo build

# Run specific component
cargo run -p quark_bot
cargo run -p quark_server
cargo run -p quark_consumer

# Run with release optimizations
cargo run --release -p quark_bot

# Run tests
cargo test

# Check formatting
cargo fmt
cargo clippy
```

### Web Application (Next.js)

```bash
cd quark-webhook

# Install dependencies
npm install
# or
pnpm install

# Development server
npm run dev
# or
pnpm dev

# Build for production
npm run build
npm start

# Development with HTTPS
npm run dev:https
```

## 📁 Project Structure

```
quark-reborn/
├── quark_bot/          # Main Telegram bot service
├── quark_server/       # REST API server
├── quark_consumer/     # Payment processing services
├── quark_core/         # Shared utilities library
├── quark-webhook/      # Next.js web application
├── contracts/          # Aptos smart contracts
├── docker-compose.yml  # Production Docker setup
├── docker-compose.debug.yml  # Debug Docker setup
└── env.example         # Environment variables template
```

## 🔧 Individual Component Setup

### quark_bot (Telegram Bot)

**Core Features:**
- **Advanced AI Conversations**: Context-aware AI with OpenAI integration, support for multiple models (GPT-4o, O3, O4-mini, etc.)
- **AI Tool Calling**: Built-in tools for balance queries, withdrawals, trending pools, time, Fear & Greed index, recent messages, and web search
- **Vector Stores**: Separate user and group document libraries with RAG (Retrieval-Augmented Generation) for context-aware responses

**Group Management Tools:**
- **Payment Automation**: Scheduled token payments with flexible intervals (5m to monthly), group wallet management
- **Content Moderation**: AI-powered sentinel with customizable allowed/disallowed rules, automatic muting, admin reporting
- **Custom Filters**: Trigger-response system with exact/contains/starts/ends matching, placeholder support, usage statistics
- **Document Library**: Group-specific vector stores for knowledge base management
- **DAO Governance**: Create proposals, voting systems with token-weighted votes, automated notifications
- **Welcome Messages**: Customizable member onboarding with dynamic placeholders
- **Command Settings**: Enable/disable specific commands per group
- **Summarization**: Automatic conversation summaries for context retention
- **Sponsor Settings**: Configure sponsorship and promotional features

**Payment & Blockchain:**
- Individual and group wallet management on Aptos
- Multi-token support (APT, USDC, USDT, etc.) with automatic token list updates
- Balance checking, withdrawals, and fund transfers
- Scheduled recurring payments with multiple cadences
- Payment preferences per user/group

**Automation:**
- **Scheduled Prompts**: Automated AI-generated messages with flexible timing (every 5m to monthly)
- **Scheduled Payments**: Recurring token transfers to users or addresses
- Safety validation for scheduled operations

**Run Locally:**
```bash
cargo run -p quark_bot
```

**Run with Docker:**
```bash
docker-compose up quark-bot -d
```

### quark_server (REST API)

**Features:**
- RESTful API endpoints
- Payment processing
- Administrative functions
- System monitoring

**Run Locally:**
```bash
cargo run -p quark_server
```

**Run with Docker:**
```bash
docker-compose up quark-server -d
```

**API Endpoints:**
- Server: `http://localhost:3200`
- Documentation: `http://localhost:3200/docs`

### quark_consumer (Payment Processors)

**Features:**
- Asynchronous payment processing
- Blockchain transaction handling
- Load balancing with multiple instances

**Run Locally:**
```bash
cargo run -p quark_consumer
```

**Run with Docker:**
```bash
docker-compose up quark-consumer-one quark-consumer-two -d
```

### quark-webhook (Web Application)

**Features:**
- Modern React/Next.js interface
- Wallet integration with Aptos Connect
- Account management and fund operations

**Run Locally:**
```bash
cd quark-webhook
npm install
npm run dev
```

**Run with Docker:**
```bash
docker-compose up quark-webhook -d
```

**Web Interface:**
- URL: `http://localhost:3000`

## 🗄️ Database & Storage

### Valkey (Redis-compatible)
- Message queue for payment processing
- Session storage and caching
- Port: 6379

### Sled Database
- Embedded database for user data
- Conversation history and credentials
- Local file-based storage

## 🔍 Monitoring & Debugging

### View Logs
```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f quark-bot
docker-compose logs -f quark-server

# Debug environment
docker-compose -f docker-compose.debug.yml logs -f
```

### Service Status
```bash
# Check running containers
docker-compose ps

# Check service health
docker-compose exec quark-server curl localhost:3200/health
```

### Debug with LLDB
```bash
# Start debug environment
docker-compose -f docker-compose.debug.yml up quark-bot-lldb -d

# Connect with LLDB client
lldb
(lldb) platform select remote-linux
(lldb) platform connect connect://localhost:1235
```

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test -p quark_bot

# Run with output
cargo test -- --nocapture

# Integration tests
cargo test --test integration_tests
```

## 📚 Documentation

- **[Architecture Documentation](Architecture.md)** - System design and component interactions
- **[License](LICENSE)** - Dual license (GPL-3.0 or Commercial) + Smart Contracts (CC BY-NC-ND 4.0)
- **[Consumer README](quark_consumer/README.md)** - Payment processor setup details

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## 📄 License

This project uses a multi-tier licensing approach:

### Software Components
- **GNU General Public License v3.0 (GPL-3.0)** - [Full License Text](LICENSE)
- **Commercial License** - Contact [james@sshiftgpt.com](mailto:james@sshiftgpt.com) or [spielcrypto@sshiftgpt.com](mailto:spielcrypto@sshiftgpt.com)

### Smart Contracts
- **Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 (CC BY-NC-ND 4.0)**
- Smart contracts in the `contracts/` directory are licensed under CC BY-NC-ND 4.0
- This allows sharing and adaptation for non-commercial purposes only
- **No commercial use** of smart contracts (original or modified versions) is permitted
- **No distribution** of modified smart contracts is allowed

## 🆘 Support

For support and questions:
- **Email**: [james@sshiftgpt.com](mailto:james@sshiftgpt.com)
- **Commercial License**: [spielcrypto@sshiftgpt.com](mailto:spielcrypto@sshiftgpt.com)

---

**Powered by:**
- [teloxide](https://github.com/teloxide/teloxide) - Telegram bot framework
- [open-ai-rust-responses-by-sshift](https://github.com/Singularity-Shift/openai-rust-responses-sshift) - OpenAI integration
- [sled](https://github.com/spacejam/sled) - Embedded database
- [Aptos](https://aptos.dev/) - Blockchain platform
- [Next.js](https://nextjs.org/) - React framework 