use ignore::WalkBuilder;
use std::collections::HashSet;
use tracing::{error, instrument};

use crate::config::{self, Config};

pub struct Crawl {
    crawl_config: config::Crawl,
    config: Config,
    crawled_file_types: HashSet<String>,
    crawled_all: bool,
}

impl Crawl {
    pub(crate) fn new(crawl_config: config::Crawl, config: Config) -> Self {
        Self {
            crawl_config,
            config,
            crawled_file_types: HashSet::new(),
            crawled_all: false,
        }
    }

    #[instrument(skip(self, f))]
    pub fn maybe_do_crawl(
        &mut self,
        triggered_file: Option<String>,
        mut f: impl FnMut(&config::Crawl, &str) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        if self.crawled_all {
            return Ok(());
        }

        if let Some(root_uri) = &self.config.client_params.root_uri {
            if !root_uri.starts_with("file://") {
                anyhow::bail!("Skipping crawling as root_uri does not begin with file://")
            }

            let extension_to_match = triggered_file
                .and_then(|tf| {
                    let path = std::path::Path::new(&tf);
                    path.extension().map(|f| f.to_str().map(|f| f.to_owned()))
                })
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
                            match f(&self.crawl_config, path_str) {
                                Ok(c) => {
                                    if !c {
                                        break;
                                    }
                                }
                                Err(e) => error!("{e:?}"),
                            }
                        } else {
                            match (
                                path.extension().and_then(|pe| pe.to_str()),
                                &extension_to_match,
                            ) {
                                (Some(path_extension), Some(extension_to_match)) => {
                                    if path_extension == extension_to_match {
                                        match f(&self.crawl_config, path_str) {
                                            Ok(c) => {
                                                if !c {
                                                    break;
                                                }
                                            }
                                            Err(e) => error!("{e:?}"),
                                        }
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
            } else {
                self.crawled_all = true
            }
        }
        Ok(())
    }
}
