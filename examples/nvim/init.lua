-- Set leader
vim.g.mapleader = " "
vim.g.maplocalleader = "\\"

-- The init_options
local lsp_ai_init_options = {
  -- Optional when using nvim-lspconfig, defaults to a file_store.
  memory = {
    -- It is important to use this method as `{}` will be interpreted as an array when it should be an object
    file_store = vim.fn.empty_dict()
  },
  models = {
    model1 = {
      type = "anthropic",
      chat_endpoint = "https://api.anthropic.com/v1/messages",
      model = "claude-3-5-sonnet-20240620",
      auth_token_env_var_name = "ANTHROPIC_API_KEY"
    }
  },
  actions = {
    {
      trigger = "!C",
      action_display_name = "Chat",
      model = "model1",
      parameters = {
        max_context = 4096,
        max_tokens = 4096,
        system = [[
You are an AI coding assistant. Your task is to complete code snippets. The user's cursor position is marked by \"<CURSOR>\". Follow these steps:

1. Analyze the code context and the cursor position.
2. Provide your chain of thought reasoning, wrapped in <reasoning> tags. Include thoughts about the cursor position, what needs to be completed, and any necessary formatting.
3. Determine the appropriate code to complete the current thought, including finishing partial words or lines.
4. Replace \"<CURSOR>\" with the necessary code, ensuring proper formatting and line breaks.
5. Wrap your code solution in <answer> tags.

Your response should always include both the reasoning and the answer. Pay special attention to completing partial words or lines before adding new lines of code.

<examples>
<example>
User input:
--main.py--
# A function that reads in user inpu<CURSOR>

Response:
<reasoning>
1. The cursor is positioned after \"inpu\" in a comment describing a function that reads user input.
2. We need to complete the word \"input\" in the comment first.
3. After completing the comment, we should add a new line before defining the function.
4. The function should use Python's built-in `input()` function to read user input.
5. We'll name the function descriptively and include a return statement.
</reasoning>

<answer>t
def read_user_input():
 user_input = input(\"Enter your input: \")
 return user_input
</answer>
</example>

<example>
User input:
--main.py--
def fibonacci(n):
 if n <= 1:
 return n
 else:
 re<CURSOR>


Response:
<reasoning>
1. The cursor is positioned after \"re\" in the 'else' clause of a recursive Fibonacci function.
2. We need to complete the return statement for the recursive case.
3. The \"re\" already present likely stands for \"return\", so we'll continue from there.
4. The Fibonacci sequence is the sum of the two preceding numbers.
5. We should return the sum of fibonacci(n-1) and fibonacci(n-2).
</reasoning>

<answer>turn fibonacci(n-1) + fibonacci(n-2)</answer>
</example>
</examples>
]],
        messages = {
          {
            role = "user",
            content = "{CODE}"
          }
        }
      },
      post_process = {
        extractor = "(?s)<answer>(.*?)</answer>"
      }
    }
  }
}

-- The easiest way to get started with the language server is to use the nvim-lspconfig plugin: https://github.com/neovim/nvim-lspconfig
-- Use the following snippet to configure it after installing it with the plugin manager of your choice.
-- See the nvim-lspconfig docs for the supported parameters on top of the init_options at https://github.com/neovim/nvim-lspconfig/blob/master/doc/lspconfig.txt
require('nvim-lspconfig').lsp_ai.setup {
  root_dir = vim.fn.getcwd(),
  init_options = lsp_ai_init_options,
  -- By default, the nvim-lspconfig will attach lsp-ai to every filetype which can lead to surprising results when using
  -- live completions especially when dealing with e.g. terminal windows or filetypes created by other plugins.
  -- Use the following parameter to restrict which filetypes to register lsp-ai with.
  -- filetypes = {}
}

-- Not needed it using nvim-lspconfig:
-- Start lsp-ai or attach the active instance when opening a buffer
local lsp_ai_config = {
  cmd = { 'lsp-ai' },
  root_dir = vim.fn.getcwd(),
  init_options = lsp_ai_init_options,
}
vim.api.nvim_create_autocmd("BufEnter", {
  callback = function(args)
    local bufnr = args.buf
    local client = vim.lsp.get_clients({bufnr = bufnr, name = "lsp-ai"})
    if #client == 0 then
      vim.lsp.start(lsp_ai_config, {bufnr = bufnr})
    end
  end,
})

-- Key mapping for code actions
vim.api.nvim_set_keymap('n', '<leader>c', '<cmd>lua vim.lsp.buf.code_action()<CR>', {noremap = true, silent = true})
