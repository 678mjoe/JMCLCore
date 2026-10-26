//! Preserve preference ordering: `managed` still falls back to a local JVM.
#[cfg(test)]
#[path = "existing_candidate.rs"]
mod existing_candidate;
use super::{
    detect::{self, CandidateSource, Environment, Error, Runtime},
    receipt, store,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    #[default]
    Auto,
    Local,
    Managed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Local,
    Managed,
    Forced,
}
#[derive(Debug, Clone)]
pub struct Request {
    pub required_major: u16,
    pub component: Option<String>,
    pub arch: String,
    pub policy: Policy,
    pub java_paths: Vec<String>,
    pub java_override: Option<String>,
    pub store_directory: String,
    pub allow_download: bool,
}
#[derive(Debug)]
pub struct Resolution {
    pub runtime: Runtime,
    pub source: Source,
}
fn local(env: &Environment, r: &Request) -> (Option<Runtime>, usize) {
    match detect::detect(env, &r.java_paths) {
        Ok(d) => (
            detect::select_compatible(&d, r.required_major, &r.arch).ok(),
            d.runtimes.len(),
        ),
        Err(_) => (None, 0),
    }
}
fn managed(env: &Environment, r: &Request) -> Option<Runtime> {
    let docs = store::list(&r.store_directory).ok()?;
    let os = host_os()?;
    for pass in 0..2 {
        for d in &docs {
            let receipt = &d.receipt;
            if receipt.major_version != r.required_major {
                continue;
            }
            let (ro, ra) = receipt::platform_target(&receipt.platform)?;
            if ro != os || ra != r.arch {
                continue;
            }
            if pass == 0 {
                let Some(component) = r.component.as_ref() else {
                    break;
                };
                if receipt.component.as_ref() != Some(component) {
                    continue;
                }
            }
            let path = Path::new(&r.store_directory)
                .join("runtimes")
                .join(&d.name)
                .join(&receipt.executable);
            if let Some(path) = path.to_str()
                && let Ok(runtime) = detect::probe(path, CandidateSource::Managed, env)
            {
                return Some(runtime);
            }
        }
    }
    None
}
pub fn host_os() -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        Some("windows")
    } else if cfg!(target_os = "macos") {
        Some("osx")
    } else if cfg!(target_os = "linux") {
        Some("linux")
    } else {
        None
    }
}
pub fn host_arch() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86" => Some("x86"),
        "x86_64" => Some("x86_64"),
        "arm" => Some("arm"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}
/// Downloader receives the required major and store root. It must return the
/// already-installed runtime executable path; credentials and network remain
/// outside this local policy engine. The installer owns its progress reporter.
pub fn resolve(
    env: Option<&Environment>,
    request: &Request,
    download: &mut impl FnMut(u16, &str) -> Result<String, Error>,
) -> Result<Resolution, Error> {
    if request.required_major == 0 {
        return Err("InvalidInput");
    };
    if let Some(path) = &request.java_override {
        if !request.java_paths.is_empty() || !Path::new(path).is_absolute() {
            return Err("InvalidInput");
        };
        let env = env.ok_or("EnvironmentUnavailable")?;
        let runtime =
            detect::probe(path, CandidateSource::Explicit, env).map_err(|_| "JavaNotFound")?;
        return Ok(Resolution {
            runtime,
            source: Source::Forced,
        });
    };
    let env = env.ok_or("EnvironmentUnavailable")?;
    let mut probed = 0;
    if request.policy != Policy::Managed {
        let (runtime, count) = local(env, request);
        probed += count;
        if let Some(runtime) = runtime {
            return Ok(Resolution {
                runtime,
                source: Source::Local,
            });
        }
    }
    if request.policy != Policy::Local
        && let Some(runtime) = managed(env, request)
    {
        return Ok(Resolution {
            runtime,
            source: Source::Managed,
        });
    }
    if request.policy == Policy::Managed {
        let (runtime, count) = local(env, request);
        probed += count;
        if let Some(runtime) = runtime {
            return Ok(Resolution {
                runtime,
                source: Source::Local,
            });
        }
    }
    if request.allow_download && request.policy != Policy::Local {
        let path = download(request.required_major, &request.store_directory).map_err(|e| {
            if matches!(e, "Canceled" | "OutOfMemory") {
                e
            } else {
                "JavaDownloadFailed"
            }
        })?;
        let runtime =
            detect::probe(&path, CandidateSource::Managed, env).map_err(|_| "JavaIncompatible")?;
        return Ok(Resolution {
            runtime,
            source: Source::Managed,
        });
    }
    Err(if probed > 0 {
        "JavaIncompatible"
    } else {
        "JavaNotFound"
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            required_major: 17,
            component: None,
            arch: "x86_64".into(),
            policy: Policy::Auto,
            java_paths: vec![],
            java_override: None,
            store_directory: "not-used".into(),
            allow_download: false,
        }
    }
    #[test]
    fn validation_order() {
        let mut r = request();
        r.required_major = 0;
        assert_eq!(
            resolve(None, &r, &mut |_, _| unreachable!()).unwrap_err(),
            "InvalidInput"
        );
        r.required_major = 17;
        r.java_override = Some("relative".into());
        assert_eq!(
            resolve(None, &r, &mut |_, _| unreachable!()).unwrap_err(),
            "InvalidInput"
        );
        r.java_override = None;
        assert_eq!(
            resolve(None, &r, &mut |_, _| unreachable!()).unwrap_err(),
            "EnvironmentUnavailable"
        );
    }
    #[cfg(unix)]
    #[test]
    fn managed_preference_local_fallback_and_override() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let java = dir.path().join("java");
        std::fs::write(&java,"#!/bin/sh\nprintf 'java.specification.version = 17\\nos.arch = amd64\\nsun.arch.data.model = 64\\n' >&2\n").unwrap();
        std::fs::set_permissions(&java, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut r = request();
        r.java_paths = vec![java.to_str().unwrap().into()];
        r.store_directory = dir.path().join("store").to_str().unwrap().into();
        r.policy = Policy::Managed;
        let result = resolve(Some(&Environment::new()), &r, &mut |_, _| {
            panic!("download not permitted")
        })
        .unwrap();
        assert_eq!(result.source, Source::Local);
        r.java_paths.clear();
        r.java_override = Some(java.to_str().unwrap().into());
        r.required_major = 99;
        r.arch = "arm".into();
        let result = resolve(Some(&Environment::new()), &r, &mut |_, _| unreachable!()).unwrap();
        assert_eq!(result.source, Source::Forced);
        assert_eq!(result.runtime.major_version, 17);
    }
}
