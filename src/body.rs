use crate::{Error, KuiperResult};
use std::{fs::File, io::Read, path::Path};

/// Evaluate the request body.
pub fn get_body(body_file_path: impl AsRef<Path>) -> KuiperResult<Option<String>> {
    let file = if let Some(file) = find_body_file(body_file_path)? {
        file
    } else {
        return Ok(None);
    };

    let body = read_file(file)?;

    Ok(Some(body))
}

fn find_body_file(body_file_path: impl AsRef<Path>) -> KuiperResult<Option<File>> {
    let file = match File::open(body_file_path) {
        Ok(f) => f,
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => return Ok(None),
            _ => return Err(Error::IO(e)),
        },
    };

    Ok(Some(file))
}

fn read_file(mut file: File) -> KuiperResult<String> {
    let mut buf = String::with_capacity(1024);

    file.read_to_string(&mut buf)?;

    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }

    Ok(buf)
}
