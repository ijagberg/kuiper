use crate::{Headers, KuiperResult, params::Params};
use jiff::Timestamp;
use log::error;
use std::{
    fmt::Display,
    io::{Read, Write, stdin},
};
use uuid::Uuid;

pub fn interpolation_prompt(name: &str) -> KuiperResult<String> {
    print!("enter a value for '{}'... ", name);
    std::io::stdout().flush()?;
    let mut buf = String::with_capacity(1024);
    stdin().read_to_string(&mut buf)?;
    println!();
    Ok(buf)
}

pub fn interpolation_expr(expr: &str) -> KuiperResult<String> {
    match expr {
        "uuid" => Ok(Uuid::new_v4().to_string()),
        "now" => Ok(Timestamp::now().to_string()),
        invalid => Err(InterpolationError::InvalidExpr(invalid.to_owned()).into()),
    }
}

pub fn interpolate_params(params: &mut Params) -> KuiperResult<()> {
    for (_, value) in params.iter_mut() {
        let new_value = interpolate_str(value)?;
        *value = new_value;
    }
    Ok(())
}

pub fn interpolate_headers(headers: &mut Headers) -> KuiperResult<()> {
    for (_, value) in headers.iter_mut() {
        if let Some(v) = value {
            let new_value = interpolate_str(&v.clone())?;
            *v = new_value;
        }
    }

    Ok(())
}

pub fn interpolate_body(input: &str) -> KuiperResult<String> {
    interpolate_str(input)
}

pub fn interpolate_uri(uri: &str) -> KuiperResult<String> {
    interpolate_str(uri)
}

fn interpolate_str(input: &str) -> KuiperResult<String> {
    let mut result = input.to_owned();
    for (start_idx, _) in input.match_indices("{{") {
        let (end_idx, _) = input[start_idx..]
            .match_indices("}}")
            .next()
            .ok_or(InterpolationError::InvalidFormat)?;
        let interpolated_name = &input[start_idx + 2..start_idx + end_idx];

        let (interpolation_type, name) = interpolated_name
            .split_once(':')
            .ok_or(InterpolationError::InvalidFormat)?;

        let value = match interpolation_type {
            "env" => std::env::var(name)
                .map_err(|_| InterpolationError::MissingEnvVar(name.to_string()))?,
            "expr" => interpolation_expr(name)?,
            "prompt" => interpolation_prompt(name)?,
            s => {
                error!(
                    "parsing Request from file failed, tried to interpolate the following '{}'",
                    s
                );
                return Err(InterpolationError::InvalidFormat.into());
            }
        };
        result = result.replace(&input[start_idx..start_idx + end_idx + 2], &value);
    }

    Ok(result)
}

#[derive(Debug)]
pub enum InterpolationError {
    MissingEnvVar(String),
    InvalidExpr(String),
    InvalidFormat,
}

impl std::error::Error for InterpolationError {}

impl Display for InterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                InterpolationError::InvalidExpr(e) => format!("invalid expr: '{}'", e),
                InterpolationError::InvalidFormat => "invalid format".to_string(),
                InterpolationError::MissingEnvVar(e) => format!("missing env var: '{}", e),
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn interpolation_error_test() {
        let result = interpolate_str("asd{{env:{{env:abc}}");
        assert!(
            matches!(&result, Err(Error::Interpolation(InterpolationError::MissingEnvVar(var))) if var == "{{env:abc"),
            "{:?}",
            result
        );

        let result = interpolate_str("{{e{{nv:hello}}}}");
        assert!(
            matches!(
                &result,
                Err(Error::Interpolation(InterpolationError::InvalidFormat))
            ),
            "{:?}",
            result
        );
    }
}
