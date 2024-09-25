-- Set leader
vim.g.mapleader = " "
vim.g.maplocalleader = "\\"

-- The init_options
-- It is important we parse it as json or `"file_store": {}` will be interpreted as an array when it should be an object
local lsp_ai_init_options_json = [[
{
  "memory": {
    "file_store": {}
  },
  "models": {
    "model1": {
      "type": "anthropic",
      "chat_endpoint": "https://api.anthropic.com/v1/messages",
      "model": "claude-3-5-sonnet-20240620",
      "auth_token_env_var_name": "ANTHROPIC_API_KEY"
    }
  },
  "actions": [
    {
      "trigger": "!C",
      "action_display_name": "Chat",
      "model": "model1",
      "parameters": {
        "max_context": 4096,
        "max_tokens": 4096,
        "system": "You are an AI coding assistant. Your task is to complete code snippets. The user's cursor position is marked by \"<CURSOR>\". Follow these steps:\n\n1. Analyze the code context and the cursor position.\n2. Provide your chain of thought reasoning, wrapped in <reasoning> tags. Include thoughts about the cursor position, what needs to be completed, and any necessary formatting.\n3. Determine the appropriate code to complete the current thought, including finishing partial words or lines.\n4. Replace \"<CURSOR>\" with the necessary code, ensuring proper formatting and line breaks.\n5. Wrap your code solution in <answer> tags.\n\nYour response should always include both the reasoning and the answer. Pay special attention to completing partial words or lines before adding new lines of code.\n\n<examples>\n<example>\nUser input:\n--main.py--\n# A function that reads in user inpu<CURSOR>\n\nResponse:\n<reasoning>\n1. The cursor is positioned after \"inpu\" in a comment describing a function that reads user input.\n2. We need to complete the word \"input\" in the comment first.\n3. After completing the comment, we should add a new line before defining the function.\n4. The function should use Python's built-in `input()` function to read user input.\n5. We'll name the function descriptively and include a return statement.\n</reasoning>\n\n<answer>t\ndef read_user_input():\n    user_input = input(\"Enter your input: \")\n    return user_input\n</answer>\n</example>\n\n<example>\nUser input:\n--main.py--\ndef fibonacci(n):\n    if n <= 1:\n        return n\n    else:\n        re<CURSOR>\n\n\nResponse:\n<reasoning>\n1. The cursor is positioned after \"re\" in the 'else' clause of a recursive Fibonacci function.\n2. We need to complete the return statement for the recursive case.\n3. The \"re\" already present likely stands for \"return\", so we'll continue from there.\n4. The Fibonacci sequence is the sum of the two preceding numbers.\n5. We should return the sum of fibonacci(n-1) and fibonacci(n-2).\n</reasoning>\n\n<answer>turn fibonacci(n-1) + fibonacci(n-2)</answer>\n</example>\n</examples>\n",
        "messages": [
          {
            "role": "user",
            "content": "{CODE}"
          }
        ]
      },
      "post_process": {
        "extractor": "(?s)<answer>(.*?)</answer>"
      }
    }
  ]
}
]]

-- The configuration
local lsp_ai_config = {
  cmd = { 'lsp-ai' },
  root_dir = vim.loop.cwd(),
  init_options = vim.fn.json_decode(lsp_ai_init_options_json),
}

-- Start lsp-ai when opening a buffer
vim.api.nvim_create_autocmd("BufEnter", {
  callback = function(args)
    local bufnr = args.buf
    local client = vim.lsp.get_active_clients({bufnr = bufnr, name = "lsp-ai"})
    if #client == 0 then
      vim.lsp.start(lsp_ai_config, {bufnr = bufnr})
    end
  end,
})

-- Key mapping for code actions
vim.api.nvim_set_keymap('n', '<leader>c', '<cmd>lua vim.lsp.buf.code_action()<CR>', {noremap = true, silent = true})
