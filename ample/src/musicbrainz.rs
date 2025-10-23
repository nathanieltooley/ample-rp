use serde::Deserialize;
use sys_media::MediaInfo;
use ureq::Agent;

use crate::{http, uri};

const API_PREFIX: &str = "https://musicbrainz.org/ws/2/release";
const COVER_API_PREFIX: &str = "https://coverartarchive.org/release";

#[derive(Debug, Deserialize)]
struct ReleaseQueryResponse {
    releases: Vec<Release>,
}

#[derive(Debug, Deserialize)]
struct Release {
    id: String,
}

#[derive(Debug, Deserialize)]
struct CoverArtResponse {
    images: Vec<CoverArtImage>,
}

#[derive(Debug, Deserialize)]
struct CoverArtImage {
    front: bool,
    back: bool,
    image: String,
}

impl CoverArtResponse {
    fn get_best_cover_url(&self) -> Option<String> {
        let mut backup_choices: Vec<(usize, usize)> = vec![];

        for (i, cover_art) in self.images.iter().enumerate() {
            if cover_art.front {
                return Some(self.images[i].image.clone());
            } else if cover_art.back {
                backup_choices.push((50, i));
            } else {
                backup_choices.push((25, i));
            }
        }

        backup_choices.sort();

        backup_choices.first().map(|prio_choice| self.images[prio_choice.1].image.clone())
    }
}

pub struct MBid(String);

pub struct Musicbrainz {
    agent: Agent,
}

impl Musicbrainz {
    pub fn new(agent: ureq::Agent) -> Self {
        Musicbrainz { agent }
    }

    pub fn get_release_mbid(&self, info: &MediaInfo) -> Result<MBid, ureq::Error> {
        let encoded_artist = uri::percent_encode(&info.artist_name);
        let encoded_album = uri::percent_encode(&info.album_name);
        let request_string = format!("{API_PREFIX}/?query=release:{encoded_album}%20AND%20artist:{encoded_artist}&fmt=json");

        let response = http::backoff_request(
            || self.agent.get(&request_string).call(),
            http::CollisionStrategy::StatusCode,
            http::DEFAULT_MAX_DUR,
        )?;

        let mut parsed_response: ReleaseQueryResponse = serde_json::from_str(&response)?;
        let release_id = parsed_response.releases.remove(0).id;
        Ok(MBid(release_id))
    }

    pub fn get_cover_url(&self, mbid: &MBid) -> Result<Option<String>, ureq::Error> {
        let uri = format!("{COVER_API_PREFIX}/{}", mbid.0);
        let result = http::backoff_request(|| self.agent.get(&uri).call(), http::CollisionStrategy::StatusCode, http::DEFAULT_MAX_DUR);

        match result {
            Err(err) => {
                match err {
                    ureq::Error::StatusCode(code) => {
                        // missing / does not have an image
                        if code == 404 { Ok(None) } else { Err(err) }
                    }
                    _ => Err(err),
                }
            }
            Ok(response) => {
                let parsed_response: CoverArtResponse = serde_json::from_str(&response).map_err(ureq::Error::Json)?;
                Ok(parsed_response.get_best_cover_url())
            }
        }
    }
}
