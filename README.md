# Code Context Engine

Code Context Engine (CCE) is a Rust server and CLI for indexing and searching source code.
It parses a codebase into semantic entities, converts them into natural language, and stores
embeddings in a vector database so that you can query code by intent rather than by exact text.

## Highlights

- **Semantic code search** — query with natural language (for example, "where is user authentication handled?") and get matching functions, classes, and code chunks with file paths and line ranges.
- **Hybrid retrieval** — combine vector similarity, BM25 full-text search, and graph-based relation expansion, then optionally rerank the results with an LLM.
- **Relationship graph** — built-in symbol table, call chains, callees/callers, class inheritance, and dependency queries backed by an inverted index.
- **Multi-language AST parsing** — tree-sitter based parsers for 20+ programming languages.
- **Document indexing** — Markdown, JSON, TOML, YAML, XML, and many plain-text formats are parsed with format-specific pipelines.
- **File-level summaries** — generated and indexed as their own vectors, and used to boost results from semantically relevant files.
- **Incremental updates and file watching** — re-index only what changed, or keep an index fresh with a live watcher.
- **Multiple interfaces** — a REST API, a command-line client, a Model Context Protocol (MCP) server, and a web dashboard.
- **Plugin system** — extend or override parsing, text generation, embedding generation, grouping, chunking, reranking, and more through Lua scripts or native dynamic libraries.

## How It Works

Indexing pipeline:

```
Scan directory
  -> Parse file (tree-sitter AST)
  -> Group related entities
  -> Convert to natural language (BM25 text + semantic summary)
  -> Generate summaries
  -> Batch embedding via an LLM provider
  -> Store vectors in Qdrant, metadata and relations in SQLite
```

At query time, the engine searches the vector store, the BM25 index, and the relation graph,
then merges, boosts, and optionally reranks the candidates before returning them.

Storage backends:

| Backend | Purpose |
|---------|---------|
| Qdrant | Entity, chunk, and file-summary vectors |
| Tantivy (embedded BM25) | Full-text keyword search and highlighted snippets |
| SQLite | Project metadata, entity store, relationship index, cache, and history |

## Supported Languages

Rust, Python, JavaScript, TypeScript, TSX, Java, Go, C, C++, C#, Ruby, PHP, Kotlin, Scala,
Dart, Lua, Bash, HTML, CSS, Vue, and Svelte.

Document and configuration formats include Markdown, JSON, TOML, YAML, XML, INI, CSV,
Makefile, RST, Dockerfiles, and plain log/text files.

## Requirements

- Rust 1.86 or newer (for building the binaries)
- A Qdrant instance (local or remote)
- An embedding model endpoint. OpenAI-compatible providers such as OpenAI, Azure OpenAI,
  Ollama, and SiliconFlow are supported. Chat and rerank models are optional.

## Getting Started

### 1. Build

```bash
cargo build --release
```

This produces three binaries under `target/release/`:

| Binary | Role |
|--------|------|
| `cce` | HTTP server |
| `cce-cli` | Command-line client |
| `cce-mcp` | MCP server |

### 2. Configure

Copy the example configuration and environment files:

```bash
cp config.example.toml config.toml
cp .env.example .env
```

Set your provider API keys in `.env` and make sure `config.toml` points at your Qdrant
instance and the embedding model you want to use.

```dotenv
CCE_EMB_API_KEY_SILICONFLOW=your-embedding-key
CCE_LLM_API_KEY_SILICONFLOW=your-chat-key
CCE_DB_QDRANT_URL=http://localhost:6333
```

Database connection and logging settings can be overridden with `CCE_DB_*` and `CCE_LOG_*`
environment variables. Model providers, models, and defaults live in `config.toml`.

### 3. Start the server

```bash
./target/release/cce
```

The server listens on `0.0.0.0:9000` by default. Set `CCE_CONFIG` to load a different config
file:

```bash
CCE_CONFIG=config.prod.toml ./target/release/cce
```

## Basic Usage

### Command-line client

The CLI talks to the server over HTTP. Its default server URL is `http://localhost:3000`;
override it with `-s` or the `CCE_SERVER_URL` environment variable.

```bash
# Point the CLI at the server
cce-cli -s http://localhost:9000 status

# Create a project for a codebase
cce-cli project create --path /path/to/project --name my-project

# Index it (replace 1 with the project id returned above)
cce-cli project index 1

# Search with natural language
cce-cli search query --query "where is user authentication handled?" --limit 10

# Restrict the search to a directory or entity type
cce-cli search query --query "error handling" --directory src/api --entities function,method

# Switch retrieval mode: vector, bm25, or hybrid
cce-cli search query --query "retry logic" --query-type hybrid
```

Inspect relationships and entities:

```bash
# Function details
cce-cli entity function 123

# Who calls this function, and what it calls
cce-cli entity callers 123
cce-cli entity calls 123

# Traverse the call chain (up = callers, down = callees)
cce-cli entity call-chain 123 --direction down

# Class inheritance and implementations
cce-cli entity inheritance 789
```

Keep an index up to date:

```bash
cce-cli watch start --path /path/to/project
cce-cli watch status
cce-cli watch stop
```

Use `-f json` or `-f plain` for machine-readable or pipe-friendly output.

### HTTP API

All endpoints are served under `/api` and return JSON. Indexing endpoints require a
`project_id`.

```bash
# Create a project
curl -X POST http://localhost:9000/api/project \
  -H "Content-Type: application/json" \
  -d '{"name": "My Project", "root_path": "/path/to/project", "extensions": ["rs", "py"]}'

# Index the project
curl -X POST http://localhost:9000/api/project/1/index

# Search
curl -X POST http://localhost:9000/api/search \
  -H "Content-Type: application/json" \
  -d '{"project_id": 1, "query": "user authentication", "limit": 10}'
```

Response envelope:

```json
{ "success": true, "data": { } }
```

### Web dashboard

The `frontend/` directory contains a SvelteKit dashboard for browsing projects, running
searches, exploring entities, and managing indexes.

```bash
cd frontend
npm install
npm run dev
```

The dev server listens on `http://localhost:3001` and proxies API requests to the backend
at `http://localhost:9000`.

### MCP server

`cce-mcp` exposes engine capabilities to MCP clients such as IDE assistants and agent
frameworks. It supports `stdio` and streamable HTTP transports.

```toml
[mcp]
enabled = true
transport = "stdio"
```

Available tools include `search`, `keyword_search`, `entity_callees`, `entity_callers`,
`call_chain`, `list_projects`, `get_project`, `index_project`, `incremental_index`, and
`health`. Use `enabled_tools` to expose a subset, for example to disable write operations.

## Plugins

Plugins can override or extend the indexing pipeline without changing the core binary.
Two plugin types are supported:

- **Lua scripts** executed in a sandboxed VM with memory and time limits.
- **Native dynamic libraries** loaded through a stable C ABI.

Capabilities include AST-to-NL text generation, document parsing, entity extraction,
grouping, chunking, reranking, query rewriting, result filtering, and file filtering.
Register plugins in `.cce/plugins.json`; see `docs/user-guide/plugin/` for details.

## Documentation

Detailed guides live in `docs/`:

- `docs/app/api/` — HTTP API reference
- `docs/architecture/` — system design and data flow
- `docs/user-guide/plugin/` — plugin development and capability reference
- `crates/app/cce-cli/README.md` — full CLI command reference

## License

GPL-3.0. See `LICENSE` for details.
