use std::cmp::Ordering;
use std::str::FromStr;

use wasm_bindgen::prelude::*;

use crate::exver;

fn js_parse<T>(s: &str) -> Result<T, JsValue>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    s.parse()
        .map_err(|e| format!("{}", e))
        .map_err(JsValue::from)
}

#[wasm_bindgen]
pub fn flavor(version: &str) -> Result<JsValue, JsValue> {
    js_parse::<exver::ExtendedVersion>(version).map(|v| {
        v.flavor()
            .map(wasm_bindgen::intern)
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL)
    })
}

#[wasm_bindgen]
pub fn compare(lhs: &str, rhs: &str) -> Result<JsValue, JsValue> {
    let s = js_parse::<exver::ExtendedVersion>(lhs).map_err(JsValue::from)?;
    let t = js_parse::<exver::ExtendedVersion>(rhs).map_err(JsValue::from)?;
    Ok(s.partial_cmp(&t)
        .map(|cmp| match cmp {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        })
        .map(JsValue::from)
        .unwrap_or(JsValue::NULL))
}

#[wasm_bindgen]
pub fn satisfies(version: &str, range: &str) -> Result<bool, JsValue> {
    let v = js_parse::<exver::ExtendedVersion>(version).map_err(JsValue::from)?;
    let r = js_parse::<exver::VersionRange>(range).map_err(JsValue::from)?;
    Ok(v.satisfies(&r))
}
