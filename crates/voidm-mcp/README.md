# voidm-mcp

Standalone MCP (Model Context Protocol) server for voidm memory system.

## Features

- **Memory Management**: Add, retrieve, list, update, and delete memories
- **Relationships**: Link/unlink memories with typed relationships
- **Ontology**: Create concepts and build knowledge graphs
- **Search**: Full-text search, fuzzy matching, and semantic search
- **Resources**: Access memory data via resource templates
- **Tools**: MCP tools for programmatic memory operations

## MCP Tools

The server exposes tools for:
- `add_memory`: Create new memory with type, content, and tags
- `get_memory`: Retrieve memory by ID
- `list_memories`: List recent memories
- `search_memories`: Search across memory corpus
- `link_memories`: Create edges between memories
- `add_concept`: Create ontology concepts
- And many more (complete list in tool router)

## MCP Resources

Resources provide read-only access to memory data:
- `memory://all`: All memories
- `memory://recent`: Recent memories
- `memory://search`: Search results
- `memory://concepts`: Ontology concepts
- `memory://graph`: Memory relationships

## Usage

```rust
use voidm_mcp::VoidmMcpServer;
use sqlx::sqlite::SqlitePool;
use voidm_core::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up your pool and config
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let config = Config::default();
    
    // Create and run server
    let server = VoidmMcpServer::new(pool, config);
    let running = server.serve(rmcp::transport::stdio()).await?;
    running.waiting().await?;
    
    Ok(())
}
```

## Protocol Support

- **Transport**: stdio only (v1)
- **Capabilities**: 
  - Tools (50+ operations)
  - Resources (read-only access)
- **Future**: Support for SSE, HTTPS transports

## Architecture

The server implements the MCP `ServerHandler` trait, providing:

1. **Tool Router**: Maps tool calls to handler functions
2. **Resource System**: Exposes structured data via resource URIs
3. **Memory Operations**: Direct integration with voidm-core CRUD
4. **Ontology Support**: Full concept graph management
5. **Search Integration**: All search modes available

## Dependencies

- `rmcp`: MCP protocol implementation
- `tokio`: Async runtime
- `sqlx`: Database access
- `serde_json`: JSON serialization
- `voidm-core`: Memory system core

## Integration with voidm-cli

Can be invoked from voidm-cli:
```bash
voidm mcp --transport stdio
```

Or run standalone for independent MCP service.

## Extension Points

The server can be extended with:
- Custom tools via `tool_handler!` macro
- Additional resources via `raw_template()`
- Custom search modes
- Specialized concept operations

## Protocol Specification

Implements Model Context Protocol v1.0:
- https://spec.modelcontextprotocol.io/

Supports MCP clients like:
- Claude Desktop
- Custom MCP clients
- Any tool supporting MCP transport
