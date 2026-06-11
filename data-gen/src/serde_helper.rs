//! Helper functions for dealing with serde_json.

use std::{any::type_name, error::Error, str::FromStr};

use anyhow::{Context, bail};
use serde_json::{Map, Value};

pub fn map_get_string(object: &Map<String, Value>, key: &str) -> anyhow::Result<String> {
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    value
        .as_str()
        .map(String::from)
        .with_context(|| format!("value type of {key} is not a string"))
}

pub fn map_get_object<'json>(
    object: &'json Map<String, Value>,
    key: &str,
) -> anyhow::Result<&'json Map<String, Value>> {
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    value
        .as_object()
        .with_context(|| format!("value type of {key} is not a object"))
}

pub fn map_get_array<'json>(
    object: &'json Map<String, Value>,
    key: &str,
) -> anyhow::Result<&'json Vec<Value>> {
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    value
        .as_array()
        .with_context(|| format!("value type of {key} is not a array"))
}

pub fn map_get_uint(object: &Map<String, Value>, key: &str) -> anyhow::Result<u64> {
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    value
        .as_u64()
        .with_context(|| format!("value type of {key} is not a u64"))
}

pub fn map_get_bool(object: &Map<String, Value>, key: &str) -> anyhow::Result<bool> {
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    value
        .as_bool()
        .with_context(|| format!("value type of {key} is not a bool"))
}

pub fn map_get_and_parse_str<T>(object: &Map<String, Value>, key: &str) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
{
    let value = object
        .get(key)
        .with_context(|| format!("no key called {key}"))?;
    let str = value
        .as_str()
        .with_context(|| format!("value type of {key} not a string"))?;

    match T::from_str(str) {
        Ok(t) => Ok(t),
        Err(err) => bail!("error parsing {ty}: {err}", ty = type_name::<T>()),
    }
}
