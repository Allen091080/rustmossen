# Mossen Launch Kit

[English](LAUNCH.md) | [简体中文](LAUNCH.zh-CN.md)

This file contains copy-ready material for announcing Mossen V1.0.0.

## Positioning

Mossen is a local-first, Rust-native terminal coding agent for developers who want a Claude Code-style workflow with their own model providers.

## Short Description

Mossen is an open-source terminal coding agent written in Rust. It supports conversational coding, file edits, permissioned shell execution, slash commands, context management, MCP, sub-agents, and configurable model providers.

## One-Line Pitch

Mossen is a Rust-native, local-first terminal coding agent with Claude Code-style workflows and user-controlled model providers.

## Show HN Draft

Title:

```text
Show HN: Mossen - a Rust-native open-source terminal coding agent
```

Post:

```text
Hi HN,

I built Mossen, a local-first terminal coding agent written in Rust.

It implements the core Claude Code-style workflow: conversational coding, file editing, permissioned shell execution, slash commands, context management, MCP integration, sub-agents, and terminal rendering.

The main design goal is provider flexibility. Mossen supports OpenAI-compatible Chat Completions, OpenAI Responses-compatible endpoints, and Anthropic-compatible endpoints. API keys stay in the user's local ~/.mossen/settings.json file rather than in the repository.

V1.0.0 is a source release focused on the core workflow. Packaging and installer polish can improve next, but the current priority is that developers can build it from source, configure their own model provider, and use the terminal coding-agent workflow locally.

GitHub: https://github.com/Allen091080/rustmossen
Configuration docs: https://github.com/Allen091080/rustmossen/blob/main/docs/CONFIGURATION.md

I'd be especially interested in feedback from people who use terminal coding agents daily, run local or OpenAI-compatible model endpoints, or care about Rust-based TUI/runtime architecture.
```

## Reddit Draft

Title:

```text
Mossen: an open-source Rust terminal coding agent with local-first model configuration
```

Post:

```text
I released Mossen V1.0.0, a Rust-native terminal coding agent.

It provides a Claude Code-style workflow in a local CLI: conversational coding, file editing, permissioned shell execution, slash commands, context management, MCP integration, sub-agents, and terminal rendering.

The project is model-provider flexible:
- OpenAI-compatible Chat Completions
- OpenAI Responses-compatible endpoints
- Anthropic-compatible endpoints

Credentials are stored locally in ~/.mossen/settings.json and are not part of the repository.

Repo: https://github.com/Allen091080/rustmossen
Docs: https://github.com/Allen091080/rustmossen/blob/main/README.md

V1.0.0 is a source release. I would appreciate feedback on buildability, provider compatibility, terminal UX, and the Rust architecture.
```

## X / Twitter Draft

```text
I open-sourced Mossen V1.0.0: a Rust-native terminal coding agent.

It brings Claude Code-style workflows to a local-first CLI:
- file edits
- shell execution with permissions
- slash commands
- context management
- MCP
- sub-agents
- configurable model providers

https://github.com/Allen091080/rustmossen
```

## Product Hunt Draft

Tagline:

```text
A Rust-native terminal coding agent with local-first model control
```

Description:

```text
Mossen is an open-source terminal coding agent written in Rust. It provides conversational coding, file editing, shell execution with permissions, slash commands, MCP integration, sub-agents, and terminal rendering. Users configure their own OpenAI-compatible, OpenAI Responses-compatible, or Anthropic-compatible model providers, with credentials stored locally.
```

## Demo Script

Use this outline for a 60-90 second GIF or video:

1. Clone the repository and build Mossen.
2. Add a model profile with placeholder credentials.
3. Run `mossen` and open the TUI.
4. Ask Mossen to inspect a small repository.
5. Let it edit a file and run a test command.
6. Show a slash command such as `/help` or `/model`.
7. End on the GitHub README and configuration docs.

Do not show real API keys, private repository paths, private transcripts, or provider-specific private output.

## Suggested GitHub Topics

```text
rust
cli
terminal
tui
coding-agent
developer-tools
llm
mcp
openai-compatible
local-first
```

## Launch Checklist

- Add a short screen recording or GIF to the README.
- Confirm CI is green on `main`.
- Create the `v1.0.0` GitHub release.
- Enable GitHub Discussions if you want public Q&A.
- Add repository topics and a concise repository description.
- Watch issues closely for the first week after launch.
