use minijinja::{context, Environment, ErrorKind};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use crate::configuration::ChatMessage;

static MINININJA_ENVIRONMENT: Lazy<Mutex<Environment>> =
    Lazy::new(|| Mutex::new(Environment::new()));

fn template_name_from_template_string(template: &str) -> String {
    xxhash_rust::xxh3::xxh3_64(template.as_bytes()).to_string()
}

pub fn apply_chat_template(
    template: &str,
    chat_messages: Vec<ChatMessage>,
    bos_token: &str,
    eos_token: &str,
) -> anyhow::Result<String> {
    let template_name = template_name_from_template_string(template);
    let mut env = MINININJA_ENVIRONMENT.lock();
    let template = match env.get_template(&template_name) {
        Ok(template) => template,
        Err(e) => match e.kind() {
            ErrorKind::TemplateNotFound => {
                env.add_template_owned(template_name.clone(), template.to_owned())?;
                env.get_template(&template_name)?
            }
            _ => anyhow::bail!(e.to_string()),
        },
    };
    Ok(template.render(
        context!(messages => chat_messages, bos_token => bos_token, eos_token => eos_token),
    )?)
}
