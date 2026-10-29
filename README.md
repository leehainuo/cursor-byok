<div align="center">

# cursor-byok

Bring your own model APIs to Cursor.

[Download](https://github.com/leehainuo/cursor-byok/releases) · [Report an Issue](https://github.com/leehainuo/cursor-byok/issues) · [中文说明](./README-CN.md)

[![Release](https://img.shields.io/github/v/release/leehainuo/cursor-byok?style=flat-square)](https://github.com/leehainuo/cursor-byok/releases)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](./LICENSE)
![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey?style=flat-square)

</div>

![Connect cursor-byok to a wide range of model APIs](./images/en-brand-1.png)

![cursor-byok dashboard](./images/en-home-1.png)

## About

cursor-byok is an open-source local model gateway for Cursor. Connect your own OpenAI- and Anthropic-compatible APIs while keeping Agent capabilities such as tool calling, Skills, MCP, and multi-turn conversations.

## Features

- **Your own models:** Configure API endpoints, keys, model IDs, and request parameters.
- **Model management:** Add, edit, reorder, and batch-test model configurations.
- **Connection benchmarks:** Measure response latency and generation speed.
- **Session metrics:** Track token usage, cache hits, and conversation activity.
- **Cross-platform:** Available for macOS, Windows, and Linux.

![cursor-byok model settings](./images/en-model-1.png)

## How It Works

```text
Cursor client
    │
    │ Agent requests and tool results
    ▼
cursor-byok local service
    │
    │ OpenAI- / Anthropic-compatible requests
    ▼
Your model API
```

cursor-byok handles protocol adaptation, model request forwarding, tool-call coordination, and conversation state on your machine. API keys and application settings are stored locally; requests are still sent to the model provider you configure.

## Why This Project

Many Agent products bundle their tool capabilities with a fixed set of models, subscriptions, and billing options, leaving users limited to the channels offered by the platform.

cursor-byok is built to return model choice to the user. Developers can make full use of the APIs and credits they already have, choose the models and providers that fit their needs, and self-host related services when required.

## Acknowledgements

Based on [leookun/cursor-byok](https://github.com/leookun/cursor-byok), independently maintained by [leehainuo](https://github.com/leehainuo). Thanks to the original author and all contributors.

Released under the [MIT License](./LICENSE). Upstream copyright and license notices are preserved in [Third-Party Licenses](./THIRD-PARTY-LICENSES.md).
