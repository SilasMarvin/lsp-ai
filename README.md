<div align="center">
   <picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://github.com/user-attachments/assets/7849b743-a3d5-4fde-8ac7-960205c1b019">
  <source media="(prefers-color-scheme: light)" srcset="https://github.com/user-attachments/assets/7903b3c2-a5ac-47e0-ae23-bd6a47b864ee">
  <img alt="Logo" src="" width="650em">
   </picture>
</div>

<p align="center">
   <p align="center"><b>Empowering not replacing programmers.</b></p>
</p>

<p align="center">
| <a href="https://github.com/SilasMarvin/lsp-ai/wiki"><b>Documentation</b></a> | <a href="https://silasmarvin.dev"><b>Blog</b></a> | <a href="https://discord.gg/vKxfuAxA6Z"><b>Discord</b></a> |
</p>

---

LSP-AI is an open source [language server](https://microsoft.github.io/language-server-protocol/) that serves as a backend for AI-powered functionality in your favorite code editors. It offers features like in-editor chatting with LLMs and code completions. Because it is a language server, it works with any editor that has LSP support.

**The goal of LSP-AI is to assist and empower software engineers by integrating with the tools they already know and love, not replace software engineers.**

A short list of a few of the editors it works with:
- VS Code
- NeoVim
- Emacs
- Helix
- Sublime

It works with many many many more editors.

# Features

## In-Editor Chatting

Chat directly in your codebase with your favorite local or hosted models.

![in-editor-chatting](https://github.com/user-attachments/assets/c69a9dc0-c0ac-4786-b24b-f5b5d19ffd3a)

*Chatting with Claude Sonnet in Helix*

## Code Completions

LSP-AI can work as an alternative to Github Copilot.

https://github.com/SilasMarvin/lsp-ai/assets/19626586/59430558-da23-4991-939d-57495061c21b

*On the left: VS Code using Mistral Codestral. On the right: Helix using stabilityai/stable-code-3b*

**Note that speed for completions is entirely dependent on the backend being used. For the fastest completions we recommend using either a small local model or Groq.**

# Documentation

See the wiki for instructions on:
- [Getting Started](https://github.com/SilasMarvin/lsp-ai/wiki)
- [Installation](https://github.com/SilasMarvin/lsp-ai/wiki/Installation)
- [Configuration](https://github.com/SilasMarvin/lsp-ai/wiki/Configuration)
- [Plugins](https://github.com/SilasMarvin/lsp-ai/wiki/Plugins)
- [Server Capabilities](https://github.com/SilasMarvin/lsp-ai/wiki/Server-Capabilities-and-Functions)
- [and more](https://github.com/SilasMarvin/lsp-ai/wiki)

# The Case for LSP-AI

**tl;dr LSP-AI abstracts complex implementation details from editor specific plugin authors, centralizing open-source development work into one shareable backend.**

Editor integrated AI-powered assistants are here to stay. They are not perfect, but are only improving and [early research is already showing the benefits](https://arxiv.org/pdf/2206.15331). While several companies have released advanced AI-powered editors like [Cursor](https://cursor.sh/), the open-source community lacks a direct competitor.

LSP-AI aims to fill this gap by providing a language server that integrates AI-powered functionality into the editors we know and love. Here’s why we believe LSP-AI is necessary and beneficial:

1. **Unified AI Features**:
    - By centralizing AI features into a single backend, LSP-AI allows supported editors to benefit from these advancements without redundant development efforts.

2. **Simplified Plugin Development**:
    - LSP-AI abstracts away the complexities of setting up LLM backends, building complex prompts and soon much more. Plugin developers can focus on enhancing the specific editor they are working on, rather than dealing with backend intricacies.

3. **Enhanced Collaboration**:
    - Offering a shared backend creates a collaborative platform where open-source developers can come together to add new functionalities. This unified effort fosters innovation and reduces duplicated work.

4. **Broad Compatibility**:
    - LSP-AI supports any editor that adheres to the Language Server Protocol (LSP), ensuring that a wide range of editors can leverage the AI capabilities provided by LSP-AI.

5. **Flexible LLM Backend Support**:
    - Currently, LSP-AI supports llama.cpp, Ollama, OpenAI-compatible APIs, Anthropic-compatible APIs, Gemini-compatible APIs and Mistral AI FIM-compatible APIs, giving developers the flexibility to choose their preferred backend. This list will soon grow.

6. **Future-Ready**:
    - LSP-AI is committed to staying updated with the latest advancements in LLM-driven software development.

# Roadmap

There is so much to do for this project and incredible new research and tools coming out everyday. Below is a list of some ideas for what we want to add next, but we welcome any contributions and discussion around prioritizing new features.

- Implement semantic search-powered context building (This could be incredibly cool and powerful). Planning to use [Tree-sitter](https://tree-sitter.github.io/tree-sitter/) to chunk code correctly.
- Support for additional backends
- Exploration of agent-based systems
