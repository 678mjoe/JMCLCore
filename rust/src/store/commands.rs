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
pub fn handle(method: &str, params: Option<&RawValue>) -> Option<Result<Value, RpcError>> {
    if method != "store.gc" {
        return None;
    }
    Some((|| {
        let p: Params = if let Some(raw) = params {
            if !raw.get().trim_start().starts_with('{') {
                return Err(RpcError::new(
                    "INVALID_PARAMS",
                    "store.gc parameters are invalid",
                ));
            }
            serde_json::from_str(raw.get())
                .map_err(|_| RpcError::new("INVALID_PARAMS", "store.gc parameters are invalid"))?
        } else {
            Params::default()
        };
        if p.store_directory.is_empty() {
            return Err(RpcError::new(
                "INVALID_PARAMS",
                "store.gc store_directory must not be empty",
            ));
        }
        let store = Store::open_existing(&p.store_directory)
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
                        // Zig parseInt accepts separators, but not a trailing separator.
                        if value.ends_with('_') || value.starts_with('_') {
                            return None;
                        }
                        let digits = value.replace('_', "");
                        p.min_age_seconds = if let Some(negative) = digits.strip_prefix('-') {
                            let n: u64 = negative.parse().ok()?;
                            if n != 0 {
                                return None;
                            }
                            n
                        } else {
                            digits.parse().ok()?
                        };
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
