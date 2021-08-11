use std::time::Duration;

use backoff::{backoff::Backoff, ExponentialBackoff};
use reqwest::blocking::Response;
use thiserror::Error;
fn main() -> Result<(), MyError> {
    match fetch_url("https://getstatuscode.com/500") {
        Ok(response) => println!("{}", response.text().unwrap()),
        Err(e) => eprintln!("{}", e),
    }
    Ok(())
}

#[derive(Error, Debug)]
enum MyError {
    #[error(transparent)]
    PermanentReqwestError { source: reqwest::Error },

    #[error("The request failed {tries} times: {source}")]
    TransientReqwestError { tries: i32, source: reqwest::Error },
}

fn fetch_url(url: &str) -> Result<Response, MyError> {
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
    use mockito::mock;

    use crate::MyError;
    #[test]
    fn test_server_error() {
        let server_url = mockito::server_url();
        let _ = mock("GET", "/server-error").with_status(500).create();
        let response = fetch_url(&format!("{}/server-error", server_url));
        handle_response(response)
    }

    fn handle_response(response: Result<Response, MyError>) {
        match response {
            Ok(response) => println!("{}", response.text().unwrap()),
            Err(e) => eprintln!("{}", e),
        }
    }
}
