use backoff::ExponentialBackoff;
use reqwest::blocking::Response;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error(transparent)]
    PermanentReqwestError { source: reqwest::Error },

    #[error("The request failed {tries} times: {source}")]
    TransientReqwestError { tries: i32, source: reqwest::Error },
}

pub fn fetch_url(url: &str) -> Result<Response, MyError> {
    let mut tries = 0;
    let fetch_operation = || {
        tries += 1;
        eprintln!("INFO: fetching {}", url);
        let response = reqwest::blocking::get(url).map_err(|e| backoff::Error::Permanent(e))?;
        if let Err(status_error) = response.error_for_status_ref() {
            let response_status = status_error.status().unwrap();
            if response_status.is_server_error() {
                eprintln!("WARN: fetch failed with status {}", response_status);
                Err(backoff::Error::Transient(status_error))
            } else {
                Err(backoff::Error::Permanent(status_error))
            }
        } else {
            Ok(response)
        }
    };

    let backoff_strategy = ExponentialBackoff {
        max_elapsed_time: Some(Duration::from_secs(2)),
        ..Default::default()
    };

    backoff::retry(backoff_strategy, fetch_operation).map_err(|e| match e {
        backoff::Error::Permanent(reqwest_error) => MyError::PermanentReqwestError {
            source: reqwest_error,
        },
        backoff::Error::Transient(reqwest_error) => MyError::TransientReqwestError {
            tries,
            source: reqwest_error,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    #[test]
    fn test_successful_response() {
        let server = MockServer::start();
        let not_found_path = "/throw-me-a-frickin-bone-here";
        let not_found_mock = server.mock(|when, then| {
            when.method(GET).path(not_found_path);
            then.status(200).body("I'm the boss. I need the info.");
        });

        let response = fetch_url(&server.url(not_found_path));

        let mock_hits = not_found_mock.hits();

        if mock_hits != 1 {
            panic!("The request was never handled.");
        }

        assert!(response.is_ok())
    }

    #[test]
    fn test_unrecoverable_server_error() {
        let server = MockServer::start();
        let internal_server_error_path = "/this-is-me-in-a-nutshell";
        let internal_server_error_mock = server.mock(|when, then| {
            when.method(GET).path(internal_server_error_path);
            then.status(500).body("Help! I'm in a nutshell!");
        });

        let response = fetch_url(&server.url(internal_server_error_path));

        let mock_hits = internal_server_error_mock.hits();

        if mock_hits <= 1 {
            panic!("The request was never retried.");
        }

        let error = response.expect_err("Response didn't error");
        assert!(error.to_string().contains(&format!("failed {}", mock_hits)));
    }

    #[test]
    fn test_unrecoverable_client_error() {
        let server = MockServer::start();
        let not_found_path = "/austin-powers-the-musical";
        let not_found_mock = server.mock(|when, then| {
            when.method(GET).path(not_found_path);
            then.status(404).body("pretty sure that one never happened");
        });

        let response = fetch_url(&server.url(not_found_path));

        let mock_hits = not_found_mock.hits();

        if mock_hits != 1 {
            panic!("The request was never handled.");
        }

        let error = response.expect_err("Response didn't error");
        assert!(error.to_string().contains("Not Found"));
    }
}
