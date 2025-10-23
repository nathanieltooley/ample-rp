use std::{thread, time::Duration};

use log::{debug, error};
use serde::Deserialize;
use ureq::Body;

use ureq::http::Response;

const STARTING_DURATION: Duration = Duration::from_millis(500);
pub const DEFAULT_MAX_DUR: Duration = Duration::from_secs(5);

/// How the backoff function determines when to retry a request (i.e. rate limit, temporary issue with server)
#[derive(Debug, Clone, Copy)]
pub enum CollisionStrategy {
    /// LastFM specifies the error in a custom JSON object so we check that error
    /// code for any transient errors
    LastFM,
    /// musicbrainz and CoverArtArchive send a status code of 503 to signal
    /// rate limiting
    StatusCode,
}

#[derive(Debug, Deserialize)]
struct LastFmError {
    error: u32,
    // message: String,
}

/// Makes a retriable http request with automatic backoff
pub fn backoff_request<F: Fn() -> Result<Response<Body>, ureq::Error>>(
    request: F,
    collision_strat: CollisionStrategy,
    max_duration: Duration,
) -> Result<String, ureq::Error> {
    let mut collisions = 0;

    loop {
        let mut rep = request()?;
        let rep_string = rep.body_mut().read_to_string()?;

        debug!("response: {rep_string}");

        let mut collision_occurred = false;
        match collision_strat {
            CollisionStrategy::LastFM => {
                if rep.status().is_server_error() {
                    let err_response: LastFmError = serde_json::from_str(&rep_string).map_err(ureq::Error::Json)?;
                    error!("LastFM Error: {err_response:?}");
                    match err_response.error {
                        8 | 16 | 29 | 11 => collision_occurred = true,
                        _ => return Err(ureq::Error::StatusCode(rep.status().as_u16())),
                    }
                }
            }
            CollisionStrategy::StatusCode => {
                if rep.status().as_u16() == 503 {
                    collision_occurred = true
                } else if rep.status().is_client_error() || rep.status().is_server_error() {
                    return Err(ureq::Error::StatusCode(rep.status().as_u16()));
                }
            }
        }

        if collision_occurred {
            let backoff = backoff_formula(collisions);
            error!("Retrying http request in {}ms", backoff.as_millis());
            debug!("Collision Count: {collisions}");
            if backoff > max_duration {
                return Err(ureq::Error::StatusCode(503));
            }

            thread::sleep(backoff_formula(collisions));
            collisions += 1;
            continue;
        }

        return Ok(rep_string);
    }
}

fn backoff_formula(collisions: u32) -> Duration {
    let b: u32 = 2;
    let multiplier = b.pow(collisions);

    STARTING_DURATION.mul_f32(multiplier as f32)
}
