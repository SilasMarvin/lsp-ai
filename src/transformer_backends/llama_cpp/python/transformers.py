import sys
import os

from llama_cpp import Llama


model = None


def activate_venv(venv):
    if sys.platform in ("win32", "win64", "cygwin"):
        activate_this = os.path.join(venv, "Scripts", "activate_this.py")
    else:
        activate_this = os.path.join(venv, "bin", "activate_this.py")

    if os.path.exists(activate_this):
        exec(open(activate_this).read(), dict(__file__=activate_this))
        return True
    else:
        print(f"Virtualenv not found: {venv}", file=sys.stderr)
        return False


def set_model(filler):
    global model
    model = Llama(
        # model_path="./tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf",  # Download the model file first
        model_path="/Users/silas/Projects/Tests/lsp-ai-tests/deepseek-coder-6.7b-base.Q4_K_M.gguf",  # Download the model file first
        n_ctx=2048,  # The max sequence length to use - note that longer sequence lengths require much more resources
        n_threads=8,  # The number of CPU threads to use, tailor to your system and the resulting performance
        n_gpu_layers=35,  # The number of layers to offload to GPU, if you have GPU acceleration available
    )


def transform(input, max_tokens):
    # Simple inference example
    output = model(
        input,  # Prompt
        max_tokens=max_tokens,  # Generate up to max tokens
        # stop=[
        #     "<|EOT|>"
        # ],  # Example stop token - not necessarily correct for this specific model! Please check before using.
        echo=False,  # Whether to echo the prompt
    )
    return output["choices"][0]["text"]
