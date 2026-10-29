export const defaultCustomHeaders = {
  "User-Agent": "claude-cli/2.1.177 (external, cli)",
  "anthropic-beta": "claude-code-20250219,context-1m-2025-08-07,interleaved-thinking-2025-05-14,redact-thinking-2026-02-12,context-management-2025-06-27,prompt-caching-scope-2026-01-05,mid-conversation-system-2026-04-07,effort-2025-11-24",
};

export const defaultCustomHeadersText = JSON.stringify(defaultCustomHeaders, null, 2);
