use crate::{
    configuration::{Chat, ChatMessage, Configuration},
    tokenizer::Tokenizer,
};
use hf_hub::api::sync::{Api, ApiRepo};

// // Source: https://huggingface.co/teknium/OpenHermes-2.5-Mistral-7B/blob/main/tokenizer_config.json
// const CHATML_CHAT_TEMPLATE: &str = "{% for message in messages %}{{'<|im_start|>' + message['role'] + '\n' + message['content'] + '<|im_end|>' + '\n'}}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\n' }}{% endif %}";
// const CHATML_BOS_TOKEN: &str = "<s>";
// const CHATML_EOS_TOKEN: &str = "<|im_end|>";

// // Source: https://huggingface.co/mistralai/Mistral-7B-Instruct-v0.1/blob/main/tokenizer_config.json
// const MISTRAL_INSTRUCT_CHAT_TEMPLATE: &str = "{{ bos_token }}{% for message in messages %}{% if (message['role'] == 'user') != (loop.index0 % 2 == 0) %}{{ raise_exception('Conversation roles must alternate user/assistant/user/assistant/...') }}{% endif %}{% if message['role'] == 'user' %}{{ '[INST] ' + message['content'] + ' [/INST]' }}{% elif message['role'] == 'assistant' %}{{ message['content'] + eos_token + ' ' }}{% else %}{{ raise_exception('Only user and assistant roles are supported!') }}{% endif %}{% endfor %}";
// const MISTRAL_INSTRUCT_BOS_TOKEN: &str = "<s>";
// const MISTRAL_INSTRUCT_EOS_TOKEN: &str = "</s>";

// // Source: https://huggingface.co/mistralai/Mixtral-8x7B-Instruct-v0.1/blob/main/tokenizer_config.json
// const MIXTRAL_INSTRUCT_CHAT_TEMPLATE: &str = "{{ bos_token }}{% for message in messages %}{% if (message['role'] == 'user') != (loop.index0 % 2 == 0) %}{{ raise_exception('Conversation roles must alternate user/assistant/user/assistant/...') }}{% endif %}{% if message['role'] == 'user' %}{{ '[INST] ' + message['content'] + ' [/INST]' }}{% elif message['role'] == 'assistant' %}{{ message['content'] + eos_token}}{% else %}{{ raise_exception('Only user and assistant roles are supported!') }}{% endif %}{% endfor %}";

pub struct Template {
    configuration: Configuration,
}

// impl Template {
//     pub fn new(configuration: Configuration) -> Self {
//         Self { configuration }
//     }
// }

pub fn apply_prompt(
    chat_messages: Vec<ChatMessage>,
    chat: &Chat,
    tokenizer: Option<&Tokenizer>,
) -> anyhow::Result<String> {
    // If we have the chat template apply it
    // If we have the chat_format see if we have it set
    // If we don't have the chat_format set here, try and get the chat_template from the tokenizer_config.json file
    anyhow::bail!("Please set chat_template or chat_format. Could not find the information in the tokenizer_config.json file")
}
