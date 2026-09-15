# AI Architecture

## Overview

The AI chat system uses a dual-adapter architecture that separates two concerns:
- **AI Provider**: Which LLM service to use (OpenAI, Ollama, etc.)
- **Memory Provider**: How to prepare conversation context (SimpleMemory, SlidingWindow, etc.)

## Architecture

```
┌─────────────────────────────────────────────────┐
│              api::generate_reply                │
│         (Flutter Bridge Entry Point)            │
└────────────────┬────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────┐
│      ai::conversation::generate_conversation    │
│              _reply (Orchestrator)              │
└─────┬───────────────────────────────────┬───────┘
      │                                   │
      ▼                                   ▼
┌─────────────────┐              ┌────────────────┐
│ MemoryProvider  │              │  AiProvider    │
│   (trait)       │              │    (trait)     │
└────┬────────────┘              └────┬───────────┘
     │                                │
     ├─ SimpleMemory                  ├─ OpenAiCompatibleProvider
     │  (last N messages)             │  (OpenAI, DeepSeek, vLLM)
     │                                │
     └─ SlidingWindowMemory           └─ OllamaProvider
        (token-limited)                  (Local Ollama)
```

## Memory Providers

### SimpleMemory
- Takes the most recent N messages
- Simplest approach, good for short conversations
- Configuration: `max_messages`

### SlidingWindowMemory
- Takes messages within a token budget
- Uses rough estimate: 1 token ≈ 4 characters
- Walks backward from most recent until budget exhausted
- Configuration: `max_tokens`

## AI Providers

### OpenAiCompatibleProvider
- Works with OpenAI, DeepSeek, vLLM, and other OpenAI-compatible APIs
- Uses `/v1/chat/completions` endpoint
- Configuration: `base_url`, `api_key`, `model`, `temperature`, `max_tokens`
- Reads config from database (`ai_provider_configs` table)

### OllamaProvider
- Works with local Ollama installation
- Uses `/api/chat` endpoint
- Configuration: `base_url`, `model`, `temperature`
- Falls back to environment variables (`OLLAMA_BASE_URL`, `OLLAMA_MODEL`)

## Usage from Flutter

```dart
final reply = await api.generateReply(
  conversationId: conversationId,
  providerType: 'openai_compatible', // or 'ollama'
  memoryType: 'sliding_window',      // or 'simple'
  memoryWindowSize: 4096,            // tokens for sliding_window, messages for simple
);
```

## Configuration

### Provider Configuration (Database)
The active AI provider is stored in `ai_provider_configs` table:
- `provider_type`: "openai_compatible"
- `base_url`: API endpoint
- `model`: Model identifier
- `api_key`: Authentication token

### Memory Configuration (Runtime)
Memory strategy is chosen at call time via `ConversationConfig`:
```rust
ConversationConfig {
    provider_type: ProviderType::OpenAiCompatible,
    memory_type: MemoryType::SlidingWindow { max_tokens: 4096 },
}
```

## Extension Points

### Adding a New AI Provider
1. Create provider config struct implementing `Default`
2. Create provider struct implementing `AiProvider` trait
3. Add variant to `ProviderType` enum
4. Add case to `generate_conversation_reply` match

### Adding a New Memory Strategy
1. Create memory struct implementing `MemoryProvider` trait
2. Add variant to `MemoryType` enum
3. Add case to `generate_conversation_reply` match

## Files

- `src/ai/mod.rs` - Module exports
- `src/ai/conversation.rs` - Orchestration logic
- `src/ai/memory.rs` - Memory provider trait and implementations
- `src/ai/provider.rs` - AI provider trait and implementations
- `src/api.rs` - Flutter bridge API (`generate_reply` function)
