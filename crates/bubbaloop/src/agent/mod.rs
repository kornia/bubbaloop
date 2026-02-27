/// Raw reqwest Claude API client with tool_use support.
pub mod claude;

/// Internal MCP tool dispatch — calls PlatformOperations directly.
pub mod dispatch;

/// SQLite memory layer — conversations, sensor events, schedules.
pub mod memory;
