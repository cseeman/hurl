/*
 * Hurl (https://hurl.dev)
 * Copyright (C) 2022 Orange
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *          http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
use std::collections::HashMap;
use std::time::Instant;

use crate::cli::Logger;
use crate::http;
use crate::http::ClientOptions;
use crate::runner::entry::get_entry_verbosity;
use hurl_core::ast::*;

use super::core::*;
use super::entry;

/// Runs a `hurl_file`, issue from the given `filename` file and `content`, with
/// an `http_client`. Returns a [`HurlResult`] upon completion.
///
/// `filename` and `content` are used to display line base logs (for parsing error or asserts
/// failures).
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use hurl::cli::Logger;
/// use hurl_core::parser;
/// use hurl::http;
/// use hurl::http::ContextDir;
/// use hurl::runner;
///
/// // Parse Hurl file
/// let filename = "sample.hurl";
/// let s = r#"
/// GET http://localhost:8000/hello
/// HTTP/1.0 200
/// "#;
/// let hurl_file = parser::parse_hurl_file(s).unwrap();
///
/// // Create an HTTP client
/// let client_options = http::ClientOptions::default();
/// let mut client = http::Client::new(&client_options);
/// let logger = Logger::new(false, false, filename, s);
///
/// // Define runner options
/// let variables = std::collections::HashMap::new();
/// let runner_options = runner::RunnerOptions {
///        fail_fast: false,
///        variables,
///        to_entry: None,
///        context_dir: ContextDir::default(),
///        ignore_asserts: false,
///        very_verbose: false,
///        pre_entry: None,
///        post_entry: None,
///  };
///
/// // Run the hurl file
/// let hurl_results = runner::run(
///     &hurl_file,
///     filename,
///     &mut client,
///     &runner_options,
///     &client_options,
///     &logger,
/// );
/// assert!(hurl_results.success);
/// ```
pub fn run(
    hurl_file: &HurlFile,
    filename: &str,
    http_client: &mut http::Client,
    runner_options: &RunnerOptions,
    client_options: &ClientOptions,
    logger: &Logger,
) -> HurlResult {
    let mut entries = vec![];
    let mut variables = HashMap::default();

    for (key, value) in &runner_options.variables {
        variables.insert(key.to_string(), value.clone());
    }

    let n = if let Some(to_entry) = runner_options.to_entry {
        to_entry
    } else {
        hurl_file.entries.len()
    };

    let start = Instant::now();
    for (entry_index, entry) in hurl_file
        .entries
        .iter()
        .take(n)
        .enumerate()
        .collect::<Vec<(usize, &Entry)>>()
    {
        if let Some(pre_entry) = runner_options.pre_entry {
            let exit = pre_entry(entry.clone());
            if exit {
                break;
            }
        }

        // We compute these new overridden options for this entry, before entering into the `run`
        // function because entry options can modify the logger and we want the preamble
        // "Executing entry..." to be displayed based on the entry level verbosity.
        let entry_verbosity = get_entry_verbosity(entry, &client_options.verbosity);
        let logger = &Logger::new(
            logger.color,
            entry_verbosity.is_some(),
            logger.filename,
            logger.content,
        );

        logger.debug_important(
            "------------------------------------------------------------------------------",
        );
        logger.debug_important(format!("Executing entry {}", entry_index + 1).as_str());

        let client_options = entry::get_entry_options(entry, client_options, logger);

        let entry_results = entry::run(
            entry,
            http_client,
            &mut variables,
            runner_options,
            &client_options,
            logger,
        );

        for entry_result in &entry_results {
            for e in &entry_result.errors {
                logger.error_rich(e);
            }
            entries.push(entry_result.clone());
        }

        if let Some(post_entry) = runner_options.post_entry {
            let exit = post_entry();
            if exit {
                break;
            }
        }

        if runner_options.fail_fast && !entry_results.last().unwrap().errors.is_empty() {
            break;
        }
    }

    let time_in_ms = start.elapsed().as_millis();
    let success = entries
        .iter()
        .flat_map(|e| e.errors.iter())
        .next()
        .is_none();

    let cookies = http_client.get_cookie_storage();
    let filename = filename.to_string();
    HurlResult {
        filename,
        entries,
        time_in_ms,
        success,
        cookies,
    }
}
