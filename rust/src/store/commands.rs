//! Existing-store GC entry points; never create a root as a collection side effect.
use super::{DEFAULT_DIRECTORY, GcOptions, Store};
use crate::protocol::RpcError;
use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};
use std::io::{self, Write};

const USAGE: &str = "usage: jmcl-core store gc [--store-directory PATH] [--min-age-seconds N] [--execute] [--json]\n";
fn directory() -> String {
    DEFAULT_DIRECTORY.into()
}
fn yes() -> bool {
    true
}
fn age() -> u64 {
    86400
}
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    crate::json_number::dynamic_raw(raw.get(), 64).map_err(|e| serde::de::Error::custom(e.name()))
}

#[derive(Deserialize)]
struct Params {
    #[serde(default = "directory")]
    store_directory: String,
    #[serde(default = "yes")]
    dry_run: bool,
    #[serde(default = "age", deserialize_with = "number")]
    min_age_seconds: u64,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            store_directory: directory(),
            dry_run: true,
            min_age_seconds: age(),
        }
    }
}
// RPC byte paths are independent from the CLI's String argument contract.
fn rpc_directory() -> Vec<u8> {
    DEFAULT_DIRECTORY.as_bytes().to_vec()
}
fn rpc_path<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let raw = <&RawValue>::deserialize(d)?;
    if raw.get().starts_with('"') {
        return serde_json::from_str::<String>(raw.get())
            .map(String::into_bytes)
            .map_err(serde::de::Error::custom);
    }
    let elements: Vec<&RawValue> =
        serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
    elements
        .into_iter()
        .map(|v| {
            crate::json_number::dynamic_raw(v.get(), 8)
                .map(|n| n as u8)
                .map_err(|e| serde::de::Error::custom(e.name()))
        })
        .collect()
}
#[derive(Deserialize)]
struct RpcParams {
    #[serde(default = "rpc_directory", deserialize_with = "rpc_path")]
    store_directory: Vec<u8>,
    #[serde(default = "yes")]
    dry_run: bool,
    #[serde(default = "age", deserialize_with = "number")]
    min_age_seconds: u64,
}
impl Default for RpcParams {
    fn default() -> Self {
        Self {
            store_directory: rpc_directory(),
            dry_run: true,
            min_age_seconds: age(),
        }
    }
}
fn prepared_root(p: &RpcParams) -> Result<std::ffi::OsString, RpcError> {
    super::path_from_bytes(&p.store_directory)
        .map_err(|_| RpcError::new("STORE_ROOT_ERROR", "Failed to open the artifact store"))
}
fn prepare_params(params: Option<&RawValue>) -> Result<RpcParams, RpcError> {
    let p: RpcParams = if let Some(raw) = params {
        if !raw.get().trim_start().starts_with('{') {
            return Err(RpcError::new(
                "INVALID_PARAMS",
                "store.gc parameters are invalid",
            ));
        }
        serde_json::from_str(raw.get())
            .map_err(|_| RpcError::new("INVALID_PARAMS", "store.gc parameters are invalid"))?
    } else {
        RpcParams::default()
    };
    if p.store_directory.is_empty() {
        return Err(RpcError::new(
            "INVALID_PARAMS",
            "store.gc store_directory must not be empty",
        ));
    }
    Ok(p)
}
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if method != "store.gc" {
        return None;
    }
    Some((|| {
        let p = prepare_params(params)?;
        let root = prepared_root(&p)?;
        let store = Store::open_existing(&root)
            .map_err(|_| RpcError::new("STORE_ROOT_ERROR", "Failed to open the artifact store"))?;
        let summary = store
            .collect(GcOptions {
                dry_run: p.dry_run,
                min_age_seconds: p.min_age_seconds,
            })
            .map_err(|_| {
                RpcError::new(
                    "STORE_GC_FAILED",
                    "Failed to collect artifact store objects",
                )
            })?;
        if summary.errors != 0 {
            return Err(RpcError::new(
                "STORE_GC_PARTIAL",
                "One or more artifact objects could not be collected safely",
            ));
        }
        Ok(json!({"dry_run":p.dry_run,"min_age_seconds":p.min_age_seconds,"summary":summary}))
    })())
}
pub fn cli(args: &[String], out: &mut impl Write, err: &mut impl Write) -> io::Result<u8> {
    let mut p = Params::default();
    let mut as_json = false;
    let parse = (|| -> Option<()> {
        if args.first()?.as_str() != "gc" {
            return None;
        }
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--execute" => p.dry_run = false,
                "--json" => as_json = true,
                "--store-directory" | "--min-age-seconds" => {
                    let key = &args[i];
                    i += 1;
                    let value = args.get(i)?;
                    if value.is_empty() {
                        return None;
                    }
                    if key == "--store-directory" {
                        p.store_directory = value.clone()
                    } else {
                        p.min_age_seconds = crate::cli_number::unsigned(value)?;
                    }
                }
                _ => return None,
            }
            i += 1;
        }
        Some(())
    })();
    if parse.is_none() {
        err.write_all(USAGE.as_bytes())?;
        return Ok(2);
    }
    let store = match Store::open_existing(&p.store_directory) {
        Ok(s) => s,
        Err(e) => {
            writeln!(
                err,
                "failed to open artifact store: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    let summary = match store.collect(GcOptions {
        dry_run: p.dry_run,
        min_age_seconds: p.min_age_seconds,
    }) {
        Ok(s) => s,
        Err(e) => {
            writeln!(
                err,
                "artifact collection failed: {}",
                crate::instances::cli_error_name(&e)
            )?;
            return Ok(1);
        }
    };
    if as_json {
        serde_json::to_writer(
            &mut *out,
            &json!({"protocol":1,"result":{"dry_run":p.dry_run,"min_age_seconds":p.min_age_seconds,"summary":summary}}),
        )?;
        out.write_all(b"\n")?;
    } else {
        writeln!(
            out,
            "mode\t{}\nminAgeSeconds\t{}\nscanned\t{}\neligible\t{}\ndeleted\t{}\neligibleBytes\t{}\ndeletedBytes\t{}\nlinked\t{}\nyoung\t{}\nignored\t{}\nbusy\t{}\nerrors\t{}",
            if p.dry_run { "dry-run" } else { "execute" },
            p.min_age_seconds,
            summary.objects_scanned,
            summary.objects_eligible,
            summary.objects_deleted,
            summary.bytes_eligible,
            summary.bytes_deleted,
            summary.objects_linked,
            summary.objects_young,
            summary.objects_ignored,
            summary.objects_busy,
            summary.errors
        )?;
    }
    Ok(u8::from(summary.errors != 0))
}

#[cfg(test)]
#[path = "../../tests/support/store_params.rs"]
mod params_tests;

#[cfg(test)]
mod cli_number_tests {
    use crate::cli_number::unsigned as cli_age;
    #[test]
    fn pure_decimal_age_matches_pinned_parse_int_boundary() {
        for (raw, expected) in [
            ("0", Some(0)),
            ("+0", Some(0)),
            ("-0", Some(0)),
            ("-00_0", Some(0)),
            ("1__2", Some(12)),
            ("+01_2", Some(12)),
            ("18446744073709551615", Some(u64::MAX)),
            ("00018446744073709551615", Some(u64::MAX)),
            ("18446744073709551616", None),
            ("-1", None),
            ("+_1", None),
            ("-_0", None),
            ("-+0", None),
            ("-_+0", None),
            ("++0", None),
            ("--0", None),
            ("+-0", None),
            ("", None),
            ("+", None),
            ("-", None),
            ("_", None),
            ("_0", None),
            ("0_", None),
            ("+0_", None),
            (" 0", None),
            ("0 ", None),
            ("0x0", None),
            ("0e0", None),
            ("0.0", None),
            ("０", None),
            ("1_+2", None),
            ("0\n", None),
        ] {
            assert_eq!(cli_age(raw), expected, "{raw:?}");
        }
    }
}
