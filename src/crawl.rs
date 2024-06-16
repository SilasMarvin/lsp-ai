use ignore::WalkBuilder;
use std::collections::HashSet;

use crate::config::{self, Config};

pub struct Crawl {
    crawl_config: config::Crawl,
    config: Config,
    crawled_file_types: HashSet<String>,
}

impl Crawl {
    pub fn new(crawl_config: config::Crawl, config: Config) -> Self {
        Self {
            crawl_config,
            config,
            crawled_file_types: HashSet::new(),
        }
    }

    pub fn crawl_config(&self) -> &config::Crawl {
        &self.crawl_config
    }

    pub fn maybe_do_crawl(
        &mut self,
        triggered_file: Option<String>,
        mut f: impl FnMut(&config::Crawl, &str) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if let Some(root_uri) = &self.config.client_params.root_uri {
            if !root_uri.starts_with("file://") {
                anyhow::bail!("Skipping crawling as root_uri does not begin with file://")
            }

            let extension_to_match = triggered_file
                .map(|tf| {
                    let path = std::path::Path::new(&tf);
                    path.extension().map(|f| f.to_str().map(|f| f.to_owned()))
                })
                .flatten()
                .flatten();

            if let Some(extension_to_match) = &extension_to_match {
                if self.crawled_file_types.contains(extension_to_match) {
                    return Ok(());
                }
            }

            if !self.crawl_config.all_files && extension_to_match.is_none() {
                return Ok(());
            }

            for result in WalkBuilder::new(&root_uri[7..]).build() {
                let result = result?;
                let path = result.path();
                if !path.is_dir() {
                    if let Some(path_str) = path.to_str() {
                        if self.crawl_config.all_files {
                            f(&self.crawl_config, path_str)?;
                        } else {
                            match (
                                path.extension().map(|pe| pe.to_str()).flatten(),
                                &extension_to_match,
                            ) {
                                (Some(path_extension), Some(extension_to_match)) => {
                                    if path_extension == extension_to_match {
                                        f(&self.crawl_config, path_str)?;
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }
            }

            if let Some(extension_to_match) = extension_to_match {
                self.crawled_file_types.insert(extension_to_match);
            }
        }
        Ok(())
    }
}
