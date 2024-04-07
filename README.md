# LSP-AI

LSP-AI is an open source language server that performs completion with large language models. Because it is a language server, it works with any editor that has LSP support.

A short list of a few of the editors it works with:
- VS Code
- NeoVim
- Emacs
- Helix
- Sublime
- JetBrains
- Zed

It works with many many many more editors.

# Installation

LSP-AI is entirely written in Rust. Install it on any platform with [cargo](https://doc.rust-lang.org/cargo/). Be sure to first install rust with [rustup](https://rustup.rs/).
```bash
cargo install lsp-ai
```

Install with the `llamacpp` feature to use [llama.cpp](https://github.com/ggerganov/llama.cpp). This automatically compiles with Metal integration if installing on MacOS.
```bash
cargo install lsp-ai -F llamacpp
```

Install with `llamacpp` and `cublas` feature to use [llama.cpp](https://github.com/ggerganov/llama.cpp) models with cuBlas. **This is recommended for Linux users with Nvidia GPUs**
```bash
cargo install lsp-ai -F llamacpp cublas
```

# Configuration Overview

LSP-AI has two configurables:
- The Memory Backend
- The Transformer Backend

# The Memory Backend

The memory backend is in charge of keeping track of opened files, and building the code and context for the transformer prompt. The transformer backend makes requests to the memory backend for prompt code and context. The memory backend responds with the following struct:
```rust
struct Prompt {
    pub context: String,
    pub code: String,
}
```

## File Store

File Store is the simplest memory backend. It keeps track of opened files and returns code and an empty context. It returns three variations of code:

1) By default it will return the code before the users cursor:
```python
def fib(n):
    if n == 0:
        return 0
    elif n == 1:
        return
```

2) When FIM is enabled it returns:
```python
<fim_prefix>def fib(n):
    if n == 0:
        return 0
    elif n == 1:
        return<fim_suffix>
    else:
        return fib(n-1) + fib(n-2)

# Some tests
assert fib(0) == 0
assert fib(1) == 1<fim_middle>
```

3) When chat is enabled it returns:
```python
def fib(n):
    if n == 0:
        return 0
    elif n == 1:
        return<CURSOR>
    else:
        return fib(n-1) + fib(n-2)

# Some tests
assert fib(0) == 0
assert fib(1) == 1
```

The size of the code returned is controlled by the max context of the transformer being used.

Use the File Store backend with the following configuration:
```json
{
  "memory": {
    "file_store": {}
  },
  "transformer": {...}
}
```

There are currently no configuration options for the File Store backend but that may change soon.

## PostgresML

**This memory backend is not ready for public use.**

The PostgresML autmatically splits and embeds opened files and performs semantic search to generate the prompt context. It still uses the File Store memory backend to generate the code part of the prompt. 

More information will be available here shortly.

# The Transformer Backend

The transformer backend receives completion and generation requests, makes prompt requests to the memory backend for code and context, and performs completion and generation using the code and context returned from the memory backend.

There are currently three different types of transformer backends:
- llama.cpp with Metal, cuBlas, or CPU support
- OpenAI compatible APIs
- Anthropic compatible APIs

## llama.cpp

llama.cpp is the recommended way for most users with decent hardware to run LSP-AI. 

### Example Configurations

Use llama.cpp with the following configuration:
```json
{
  "memory": {...},
  "transformer": {
    "llamacpp": {
      "repository": "stabilityai/stable-code-3b",
      "name": "stable-code-3b-Q5_K_M.gguf",
      "max_tokens": {
        "completion": 16,
        "generation": 32
      },
      "n_ctx": 2048,
      "n_gpu_layers": 1000
    }
  }
}
```

Provide `transformer->fim` to perform FIM completion and generation.
```json
{
  "memory": {...},
  "transformer": {
    "llamacpp": {
      "repository": "stabilityai/stable-code-3b",
      "name": "stable-code-3b-Q5_K_M.gguf",
      "max_tokens": {
        "completion": 16,
        "generation": 32
      },
      "fim": {
        "start": "<fim_prefix>",
        "middle": "<fim_suffix>",
        "end": "<fim_middle>"
      },
      "n_ctx": 2048,
      "n_gpu_layers": 1000
    }
  }
}
```

Provide `transformer->chat` to perform completion and generation with an instruction tuned model. **This will override FIM.**
```json
{
  "memory": {},
  "transformer": {
    "llamacpp": {
      "repository": "TheBloke/Mixtral-8x7B-Instruct-v0.1-GGUF",
      "name": "mixtral-8x7b-instruct-v0.1.Q5_0.gguf",
      "max_tokens": {
        "completion": 16,
        "generation": 32
      },
      "chat": {
        "completion": [
          {
            "role": "system",
            "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
          },
          {
            "role": "user",
            "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
          }
        ],
        "generation": [
          {
            "role": "system",
            "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
          },
          {
            "role": "user",
            "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
          }
        ],
      },
      "n_ctx": 2048,
      "n_gpu_layers": 1000
    }
  }
}
```

The placeholders `{CONTEXT}` and `{CODE}` are replaced with the context and code returned by the memory backend. The `<CURSOR>` string is inserted at the location of the user's cursor.

Provide `transformer->chat->chat_template` to use a custom chat template not provided by llama.cpp.
```json
{
  "memory": {...},
  "transformer": {
    ...
    "chat": {
      ...
      "chat_template": "{% if not add_generation_prompt is defined %}\n{% set add_generation_prompt = false %}\n{% endif %}\n{%- set ns = namespace(found=false) -%}\n{%- for message in messages -%}\n    {%- if message['role'] == 'system' -%}\n        {%- set ns.found = true -%}\n    {%- endif -%}\n{%- endfor -%}\n{{bos_token}}{%- if not ns.found -%}\n{{'You are an AI programming assistant, utilizing the Deepseek Coder model, developed by Deepseek Company, and you only answer questions related to computer science. For politically sensitive questions, security and privacy issues, and other non-computer science questions, you will refuse to answer\\n'}}\n{%- endif %}\n{%- for message in messages %}\n    {%- if message['role'] == 'system' %}\n{{ message['content'] }}\n    {%- else %}\n        {%- if message['role'] == 'user' %}\n{{'### Instruction:\\n' + message['content'] + '\\n'}}\n        {%- else %}\n{{'### Response:\\n' + message['content'] + '\\n<|EOT|>\\n'}}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{% if add_generation_prompt %}\n{{'### Response:'}}\n{% endif %}"
    },
  }
}
```

We currently use the [Mini Jinja](https://github.com/mitsuhiko/minijinja) crate to perform templating. It does not support the entire feature set of [Jinja](https://jinja.palletsprojects.com/en/3.1.x/).

### Parameter Overview

- **repository** is the HuggingFace repository the model is located in
- **name** is the name of the model file
- **max_tokens** restricts the number of tokens the model generates
- **fim** enables FIM support
- **chat** enables chat support
- **n_ctx** the maximum number of tokens to input to the model
- **n_gpu_layers** the number of layers to offload onto the GPU

## OpenAI Compatible APIs

LSP-AI works with any OpenAI compatible API. This means LSP-AI will work with OpenAI and any model hosted behind a compatible API. We recommend considering [OpenRouter](https://openrouter.ai/) and [Fireworks AI](https://fireworks.ai) for hosted model inference.

Using an API provider means parts of your code may be sent to the provider in the form of a LLM prompt. **If you do not want to potentially expose your code to 3rd parties we recommend using the llama.cpp backend.** 

### Example Configurations

Use GPT-4 with the following configuration:
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      "chat_endpoint": "https://api.openai.com/v1/chat/completions",
      "model": "gpt-4-0125-preview",
      "auth_token_env_var_name": "OPENAI_API_KEY",
      "chat": {
        "completion": [
            {
              "role": "system",
              "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
            },
            {
              "role": "user",
              "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
            }
        ],
        "generation": [
            {
              "role": "system",
              "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
            },
            {
              "role": "user",
              "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
            }
        ]
      },
      "max_tokens": {
        "completion": 16,
        "generation": 64
      },
      "max_context": 4096
    }
  }
}
```

The placeholders `{CONTEXT}` and `{CODE}` are replaced with the context and code returned by the memory backend. The `<CURSOR>` string is inserted at the location of the user's cursor.

Provide the `transformer->openai->fim` key to use a model with FIM support enabled. **Do not include `transformer->openai->chat` or it will override FIM.**
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      "completions_endpoint": "https://api.fireworks.ai/inference/v1/completions",
      "model": "accounts/fireworks/models/starcoder-16b",
      "auth_token_env_var_name": "FIREWORKS_API_KEY",
      "fim": {
        "start": "<fim_prefix>",
        "middle": "<fim_suffix>",
        "end": "<fim_middle>"
      },
      "max_tokens": {
        "completion": 16,
        "generation": 64
      },
      "max_context": 4096
    }
  }
}
```

Do not provide `transformer->openai-fim` and `transformer->openai->chat` to perform text completion.
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      "completions_endpoint": "https://api.fireworks.ai/inference/v1/completions",
      "model": "accounts/fireworks/models/starcoder-16b",
      "auth_token_env_var_name": "FIREWORKS_API_KEY",
      "max_tokens": {
        "completion": 16,
        "generation": 64
      },
      "max_context": 4096
    }
  }
}
```

Provide `transformer->openai->max_requests_per_second` to rate limit the number of requests. This can be useful if the editor has a very small delay before making a completions request to the LSP.
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      ...
      "max_requests_per_second": 0.5
    }
  }
}
```

Setting `transformer->openai->max_requests_per_second` to `0.5` restricts LSP-AI to making an API request once every 2 seconds.

## Parameter Overview

- **completions_endpoint** is the endpoint for text completion
- **chat_endpoint** is the endpoint for chat completion
- **model** specifies which model to use
- **auth_token_env_var_name** is the environment variable name to get the authentication token from. See `auth_token` for more authentication options
- **auth_token** is the authentication token to use. This can be used in place of `auth_token_env_var_name`
- **max_context** restricts the number of tokens to send with each request
- **max_tokens** restricts the number of tokens to generate
- **top_p** - see [OpenAI docs](https://platform.openai.com/docs/api-reference/chat/create)
- **presence_penalty** - see [OpenAI docs](https://platform.openai.com/docs/api-reference/chat/create)
- **frequency_penalty** - see [OpenAI docs](https://platform.openai.com/docs/api-reference/chat/create)
- **temperature** - see [OpenAI docs](https://platform.openai.com/docs/api-reference/chat/create)
- **max_requests_per_second** rate limits requests

## Anthropic Compatible APIs

LSP-AI works with any Anthropic compatible API. This means LSP-AI will work with Anthropic and any model hosted behind a compatible API.

Using an API provider means parts of your code may be sent to the provider in the form of a LLM prompt. **If you do not want to potentially expose your code to 3rd parties we recommend using the llama.cpp backend.** 

### Example Configurations

Use Claude Opus / Sonnet / Haiku with the following configuration:
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      "chat_endpoint": "https://api.anthropic.com/v1/messages",
      "model": "claude-3-haiku-20240307",
      "auth_token_env_var_name": "ANTHROPIC_API_KEY",
      "chat": {
        "completion": [
            {
              "role": "system",
              "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
            },
            {
              "role": "user",
              "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
            }
        ],
        "generation": [
            {
              "role": "system",
              "content": "You are a coding assistant. Your job is to generate a code snippet to replace <CURSOR>.\n\nYour instructions are to:\n- Analyze the provided [Context Code] and [Current Code].\n- Generate a concise code snippet that can replace the <cursor> marker in the [Current Code].\n- Do not provide any explanations or modify any code above or below the <CURSOR> position.\n- The generated code should seamlessly fit into the existing code structure and context.\n- Ensure your answer is properly indented and formatted based on the <CURSOR> location.\n- Only respond with code. Do not respond with anything that is not valid code."
            },
            {
              "role": "user",
              "content": "[Context code]:\n{CONTEXT}\n\n[Current code]:{CODE}"
            }
        ]
      },
      "max_tokens": {
        "completion": 16,
        "generation": 64
      },
      "max_context": 4096
    }
  }
}
```

The placeholders `{CONTEXT}` and `{CODE}` are replaced with the context and code returned by the memory backend. The `<CURSOR>` string is inserted at the location of the user's cursor.

We recommend using Haiku as we have found it to be relatively fast and cheap.

Provide `transformer->openai->max_requests_per_second` to rate limit the number of requests. This can be useful if the editor has a very small delay before making a completions request to the LSP.
```json
{
  "memory": {...},
  "transformer": {
    "openai": {
      ...
      "max_requests_per_second": 0.5
    }
  }
}
```

Setting `transformer->openai->max_requests_per_second` to `0.5` restricts LSP-AI to making an API request once every 2 seconds.

- **chat_endpoint** is the endpoint for chat completion
- **model** specifies which model to use
- **auth_token_env_var_name** is the environment variable name to get the authentication token from. See `auth_token` for more authentication options
- **auth_token** is the authentication token to use. This can be used in place of `auth_token_env_var_name`
- **max_context** restricts the number of tokens to send with each request
- **max_tokens** restricts the number of tokens to generate
- **top_p** - see [Anthropic docs](https://docs.anthropic.com/claude/reference/messages_post)
- **temperature** - see [Anthropic docs](https://docs.anthropic.com/claude/reference/messages_post)
- **max_requests_per_second** rate limits requests
