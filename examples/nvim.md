# Neovim configuration

## Register the language server with neovim

TODO: contribute some defaults to the nvim-lspconfig repo and link to that instead.

The following code will register the language server with Neovim, and naively attach it to every buffer. `lsp-ai` is
configured with the `llama_cpp` backend to run the CodeGemma v1.1 model.

Note: the model configuration is provided as an example/starting point only and I do not vouch for the quality of the
generations. Adjust to taste.

```lua
local lsp_ai_config = {
  -- Uncomment the following line if you use nvim-cmp with the cmp_nvim_lsp source.
  -- capabilities = require('cmp_nvim_lsp').default_capabilities(),
  cmd = { 'lsp-ai' },
  cmd_env = {
    -- Add required environment variables here, e.g. for CUDA device selection.
    -- CUDA_VISIBLE_DEVICES = "1"
  },
  root_dir = vim.loop.cwd(),
  init_options = {
    -- cmp-ai configuration goes here.
    memory = {
      file_store = {}
    },
    models = {
      model1 = {
        type = "llama_cpp",
        repository = "mmnga/codegemma-1.1-2b-gguf",
        name = "codegemma-1.1-2b-Q8_0.gguf",
        n_ctx = 2048,
        n_gpu_layers = 999
      }
    },
    completion = {
      model = "model1",
      parameters = {
        fim = {
          start = "<|fim_prefix|>",
          middle = "<|fim_suffix|>",
          ["end"] = "<|fim_middle|>"
        },
        max_context = 2000,
        max_new_tokens = 32
      }
    }
  }
}

local function attach_buffer()
  vim.lsp.start(lsp_ai_config)
end

vim.api.nvim_create_autocmd({"BufEnter", "BufWinEnter"}, {
  callback = attach_buffer
})
```

## Example ghost-text setup

For a copilot-like ghost-text experience, here is an example configuration using the
[nvim-cmp](https://github.com/hrsh7th/nvim-cmp) plugin, assuming you use the
[cmp-nvim-lsp](https://github.com/hrsh7th/cmp-nvim-lsp) source. This is **not a full configuration**, please refer to
the nvim-cmp documentation for a full starter config without ghost text if you need one.

This configuration enables ghost-text in nvim-cmp, and registers a custom comparator that puts `cmp-ai` suggestions
at the top so that they're the ones being drawn with ghost text.

```lua
local function ai_top_comparator(entry1, entry2)
  local comp_item = entry1:get_completion_item()
  if comp_item ~= nil then
    if string.sub(comp_item.label, 1, 4) == "ai -" then
      return true
    end
  end
  comp_item = entry2:get_completion_item()
  if comp_item ~= nil then
    if string.sub(comp_item.label, 1, 4) == "ai -" then
      return false
    end
  end
  return nil
end

local default_sorting = require('cmp.config.default')().sorting
local my_sorting = vim.tbl_extend("force", {}, default_sorting)
table.insert(my_sorting.comparators, 1, ai_top_comparator)

cmp.setup({
  -- <your nvim-cmp config here>
  -- ...
  experimental = {
    ghost_text = true
  },
  sorting = my_sorting
})
```

Notes and known issues:

* You'll need a very recent version of `nvim-cmp` for multiline ghost text to work. Note that ghost-text is an
  experimental feature of `nvim-cmp`.
* The completions window is currently drawn below the cursor, which hides ghost-text on the following lines. This is
  a known limitation of nvim-cmp, currently being addressed in PR 1955, so you may want to use the PR 1955 branch for
  now.
* The first character of the suggestion is not being properly drawn.
