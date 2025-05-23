use crate::{Headers, KuiperResult, Params};
use serde::Deserialize;
use std::{fs::File, io::BufReader, path::Path};

/// Evaluate a `RequestFile` from the file at the provided `path`.
pub fn get_request_file(path: impl AsRef<Path>) -> KuiperResult<RequestFile> {
    let file = File::open(path.as_ref())?;
    let reader = BufReader::new(file);
    let request: RequestFile = serde_json::from_reader(reader)?;
    Ok(request)
}

/// This is the format of a `kuiper` request file.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RequestFile {
    pub(crate) uri: String,
    pub(crate) method: String,
    pub(crate) headers: Headers,
    pub(crate) params: Params,
    #[serde(alias = "body")]
    pub(crate) body_file: Option<String>,
}
