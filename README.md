<picture>
  <source media="(prefers-color-scheme: dark)" srcset="/logos/logo-white-no-background-1024x1024.png">
  <source media="(prefers-color-scheme: light)" srcset="/logos/logo-white-black-background-1024x1024.png">
  <img alt="Logo" src="/logos/logo-white-black-background-1024x1024.png" width="128em">
</picture>

# LSP-AI

LSP-AI is an open source [language server](https://microsoft.github.io/language-server-protocol/) that serves as a backend for performing completion with large language models and soon other AI powered functionality. Because it is a language server, it works with any editor that has LSP support.

**The goal of LSP-AI is to assist and empower software engineers by integrating with the tools they already know and love not replace software engineers.**

A short list of a few of the editors it works with:
- VS Code
- NeoVim
- Emacs
- Helix
- Sublime

It works with many many many more editors.

See the wiki for instructions on:
- [Getting Started](https://github.com/SilasMarvin/lsp-ai/wiki)
- [Installation](https://github.com/SilasMarvin/lsp-ai/wiki/Installation)
- [Configuration](https://github.com/SilasMarvin/lsp-ai/wiki/Configuration)
- [Plugins](https://github.com/SilasMarvin/lsp-ai/wiki/Plugins)
- [Server Capabilities](https://github.com/SilasMarvin/lsp-ai/wiki/Server-Capabilities-and-Functions)
- [and more](https://github.com/SilasMarvin/lsp-ai/wiki)

LSP-AI can work as an alternative to Github Copilot.

https://github.com/SilasMarvin/lsp-ai/assets/19626586/59430558-da23-4991-939d-57495061c21b

*On the left: VS Code using Mistral Codestral. On the right: Helix using stabilityai/stable-code-3b*

**Note that speed for completions is entirely dependent on the backend being used. For the fastest completions we recommend using either a small local model or Groq.**

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
    - Currently, LSP-AI supports llama.cpp, OpenAI-compatible APIs, Anthropic-compatible APIs and Mistral AI FIM-compatible APIs, giving developers the flexibility to choose their preferred backend. This list will soon grow.

6. **Future-Ready**:
    - LSP-AI is committed to staying updated with the latest advancements in LLM-driven software development.

# Roadmap

There is so much to do for this project and incredible new research and tools coming out everyday. Below is a list of some ideas for what we want to add next, but we welcome any contributions and discussion around prioritizing new features.

- Implement semantic search-powered context building (This could be incredibly cool and powerful). Planning to use [Tree-sitter](https://tree-sitter.github.io/tree-sitter/) to chunk code correctly.
- Support for additional backends
- Exploration of agent-based systems
