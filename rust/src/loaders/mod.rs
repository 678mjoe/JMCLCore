//! Source-faithful Forge/NeoForge processor chain. Inputs are already downloaded
//! by the verified installer. Generated immutable output returns to the CAS.
use crate::{
    install::Error,
    java::detect,
    metadata, plan,
    store::{ArtifactRoot, Store},
    version::{fabric, neoforge as neo, zip},
};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};
mod process;
pub use process::SystemRuntime;
const ARCHIVE_LIMIT: usize = 64 * 1024 * 1024;
fn policy(e: metadata::Error) -> Error {
    Error::named(e.name())
}
#[derive(Clone, Copy, Debug)]
pub enum Loader<'a> {
    NeoForge {
        version: &'a str,
    },
    Forge {
        minecraft: &'a str,
        version: &'a str,
    },
}
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub server: bool,
    pub java_paths: Vec<String>,
    pub required_major: u16,
    pub minecraft_jar_relative: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Patched,
    Cached,
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub status: Status,
    pub processors_run: usize,
    pub patched_sha1: String,
    pub patched_size: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProcessorStarted {
    pub index: usize,
    pub total: usize,
    pub name: String,
}
/// Explicit injection seam for deterministic, offline fixtures. Production uses
/// discovery/probing rather than trusting a supplied executable without probing.
pub trait Runtime {
    fn resolve(
        &mut self,
        environment: &detect::Environment,
        paths: &[String],
        major: u16,
        arch: &str,
    ) -> Result<String, Error>;
    fn exit_code(&self) -> Option<i32> {
        None
    }
    fn stderr(&self) -> &[u8] {
        &[]
    }
    fn execute(
        &mut self,
        executable: &str,
        argv: &[OsString],
        cwd: &Path,
        environment: &detect::Environment,
    ) -> Result<(), Error>;
}
impl Loader<'_> {
    pub fn support(self, server: bool) -> Result<String, Error> {
        let suffix = if server { "-server" } else { "" };
        match self {
            Self::NeoForge { version } => {
                neo::version_id(version).map_err(policy)?;
                Ok(format!(".jmcl/neoforge{suffix}/{version}"))
            }
            Self::Forge { minecraft, version } => {
                neo::version_id(minecraft).map_err(policy)?;
                neo::version_id(version).map_err(|_| Error::named("InvalidForgeVersion"))?;
                Ok(format!(".jmcl/forge{suffix}/{minecraft}-{version}"))
            }
        }
    }
}
pub fn run(
    root: &ArtifactRoot,
    store: &Store,
    loader: Loader<'_>,
    version_id: &str,
    options: &Options,
    environment: Option<&detect::Environment>,
    reporter: &mut impl FnMut(ProcessorStarted) -> Result<(), Error>,
) -> Result<Summary, Error> {
    run_with(
        root,
        store,
        loader,
        version_id,
        options,
        environment,
        reporter,
        &mut SystemRuntime::default(),
    )
}
fn read(root: &ArtifactRoot, path: &str, limit: usize, too_large: &str) -> Result<Vec<u8>, Error> {
    let f = root.open_file(path)?;
    let mut bytes = Vec::new();
    f.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::named(too_large));
    }
    Ok(bytes)
}
#[derive(Deserialize)]
struct State {
    patched_sha1: String,
    patched_size: u64,
}
fn read_state(root: &ArtifactRoot, support: &str) -> Result<Option<State>, Error> {
    let bytes = match read(
        root,
        &format!("{support}/state.json"),
        4096,
        "StreamTooLong",
    ) {
        Err(e) if e.name() == "FileNotFound" => return Ok(None),
        other => other?,
    };
    let Ok(s) = serde_json::from_slice::<State>(&bytes) else {
        return Ok(None);
    };
    if fabric::sha1(&s.patched_sha1).is_err()
        || s.patched_size == 0
        || s.patched_size > 256 * 1024 * 1024
    {
        return Ok(None);
    }
    Ok(Some(s))
}
/// Forge's client hash is optional (including malformed/missing literals);
/// server hash is required and never falls back to the client declaration.
pub fn expected_forge_hash(
    profile: &serde_json::Value,
    server: bool,
) -> Result<Option<String>, Error> {
    let missing = || Error::named("PatchedServerShaMissing");
    let raw = profile["data"]["PATCHED_SHA"][if server { "server" } else { "client" }].as_str();
    let Some(raw) = raw else {
        return if server { Err(missing()) } else { Ok(None) };
    };
    let neo::Data::Literal(hash) = neo::classify(raw) else {
        return if server { Err(missing()) } else { Ok(None) };
    };
    if let Err(e) = fabric::sha1(hash) {
        return if server { Err(policy(e)) } else { Ok(None) };
    }
    Ok(Some(hash.into()))
}
struct Work<'a> {
    root: &'a ArtifactRoot,
    path: String,
}
impl Drop for Work<'_> {
    fn drop(&mut self) {
        let _ = self.root.remove_tree(&self.path);
    }
}
#[allow(clippy::too_many_arguments)]
pub fn run_with(
    root: &ArtifactRoot,
    store: &Store,
    loader: Loader<'_>,
    version_id: &str,
    options: &Options,
    environment: Option<&detect::Environment>,
    reporter: &mut impl FnMut(ProcessorStarted) -> Result<(), Error>,
    runtime: &mut impl Runtime,
) -> Result<Summary, Error> {
    let support = loader.support(options.server)?;
    // Forge validates/parses the installer before runLayout validates version_id.
    if !matches!(loader, Loader::Forge { .. }) {
        neo::version_id(version_id).map_err(policy)?;
    }
    let installer = format!("{support}/installer.jar");
    let bytes = read(root, &installer, ARCHIVE_LIMIT, "InstallerArchiveTooLarge").map_err(|e| {
        if e.name() == "FileNotFound" {
            Error::named("InstallerArchiveMissing")
        } else {
            e
        }
    })?;
    let source = zip::read_entry(&bytes, "install_profile.json", 16 * 1024 * 1024)
        .map_err(policy)?
        .ok_or_else(|| {
            Error::named(if matches!(loader, Loader::Forge { .. }) {
                "ForgeInstallProfileMissing"
            } else {
                "NeoForgeInstallProfileMissing"
            })
        })?;
    let profile = neo::install(&source).map_err(policy)?;
    let expected = if matches!(loader, Loader::Forge { .. }) {
        expected_forge_hash(&profile, options.server)?
    } else {
        None
    };
    neo::version_id(version_id).map_err(policy)?;
    let patched = neo::patched_path(&profile, options.server).map_err(policy)?;
    if let Some(s) = read_state(root, &support)?
        && root
            .verify(&patched, &s.patched_sha1, s.patched_size)
            .unwrap_or(false)
    {
        return Ok(Summary {
            status: Status::Cached,
            processors_run: 0,
            patched_sha1: s.patched_sha1,
            patched_size: s.patched_size,
        });
    }
    let env = environment.ok_or_else(|| Error::named("EnvironmentUnavailable"))?;
    let host = plan::Target::host().map_err(policy)?;
    let java = runtime.resolve(env, &options.java_paths, options.required_major, &host.arch)?;
    root.ensure_directory(&support)?;
    let work = format!("{support}/work");
    let _ = root.remove_tree(&work);
    root.ensure_directory(&work)?;
    let _cleanup = Work {
        root,
        path: work.clone(),
    };
    let absolute = root.absolute_path()?;
    let libraries = absolute.join("libraries");
    let minecraft = if options.server {
        "server.jar".into()
    } else {
        options
            .minecraft_jar_relative
            .clone()
            .unwrap_or_else(|| format!("versions/{version_id}/{version_id}.jar"))
    };
    if minecraft.is_empty()
        || minecraft.len() > 1024
        || minecraft.split('/').any(|c| {
            c.is_empty()
                || c == "."
                || c == ".."
                || !c.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b'\\')
        })
    {
        return Err(Error::named("InvalidRelativePath"));
    }
    let text = |p: PathBuf| p.into_os_string();
    let mut map: HashMap<String, OsString> = HashMap::from([
        (
            "SIDE".into(),
            if options.server { "server" } else { "client" }.into(),
        ),
        ("ROOT".into(), text(absolute.clone())),
        ("LIBRARY_DIR".into(), text(libraries.clone())),
        ("INSTALLER".into(), text(absolute.join(&installer))),
        ("MINECRAFT_JAR".into(), text(absolute.join(minecraft))),
    ]);
    for (key, value) in profile["data"].as_object().unwrap() {
        let raw = if options.server {
            value["server"].as_str().or(value["client"].as_str())
        } else {
            value["client"].as_str()
        }
        .unwrap();
        let value = match neo::classify(raw) {
            neo::Data::Literal(s) => s.into(),
            neo::Data::Coordinate(c) => {
                let relative = format!("libraries/{}", neo::artifact_path(c).map_err(policy)?);
                if let Some(parent) = Path::new(&relative).parent() {
                    root.ensure_directory(parent.to_str().unwrap())?;
                }
                text(absolute.join(relative))
            }
            neo::Data::Embedded(entry) => {
                let content = zip::read_entry(&bytes, entry, ARCHIVE_LIMIT)
                    .map_err(policy)?
                    .ok_or_else(|| Error::named("InstallerEmbeddedDataMissing"))?;
                let destination = format!("{work}/{key}");
                use std::io::Write;
                root.create_file(&destination)?.write_all(&content)?;
                text(absolute.join(destination))
            }
        };
        map.insert(key.clone(), value);
    }
    let applicable: Vec<_> = profile["processors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| neo::processor_applies(p, options.server))
        .collect();
    for (index, p) in applicable.iter().enumerate() {
        let name = p["jar"].as_str().unwrap();
        reporter(ProcessorStarted {
            index: index + 1,
            total: applicable.len(),
            name: name.into(),
        })?;
        let relative = neo::artifact_path(name).map_err(policy)?;
        let mut cp = vec![relative.clone()];
        for c in p["classpath"].as_array().unwrap() {
            cp.push(neo::artifact_path(c.as_str().unwrap()).map_err(policy)?)
        }
        for c in &cp {
            root.open_file(&format!("libraries/{c}"))
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        Error::named("ProcessorDependencyMissing")
                    } else {
                        e.into()
                    }
                })?
                .metadata()?;
        }
        let jar = read(
            root,
            &format!("libraries/{relative}"),
            ARCHIVE_LIMIT,
            "ProcessorJarTooLarge",
        )?;
        let main = main_class(&jar)?;
        let mut classpath = OsString::new();
        for (i, path) in cp.iter().enumerate() {
            if i > 0 {
                classpath.push(if cfg!(windows) { ";" } else { ":" })
            }
            classpath.push(libraries.join(path));
        }
        let mut argv = vec![OsString::from("-cp"), classpath, main.into()];
        for a in p["args"].as_array().unwrap() {
            argv.push(substitute_os(a.as_str().unwrap(), &map, &libraries)?)
        }
        if let Err(error) = runtime.execute(&java, &argv, &absolute, env) {
            if error.name() == "ProcessorFailed"
                && let Some(code) = runtime.exit_code()
            {
                let loader_name = if matches!(loader, Loader::Forge { .. }) {
                    "forge"
                } else {
                    "neoforge"
                };
                let mut stderr = std::io::stderr();
                let _ = std::io::Write::write_fmt(
                    &mut stderr,
                    format_args!("{loader_name} processor {name} exited {code}\n"),
                );
                let _ = std::io::Write::write_all(&mut stderr, runtime.stderr());
                let _ = std::io::Write::write_all(&mut stderr, b"\n");
            }
            return Err(error);
        }
    }
    let mut f = root.open_file(&patched).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::named("PatchedClientMissing")
        } else {
            e.into()
        }
    })?;
    let mut hash = Sha1::new();
    std::io::copy(&mut f, &mut hash)?;
    let size = f.metadata()?.len();
    let hash = format!("{:x}", hash.finalize());
    if expected.is_some_and(|expected| !expected.eq_ignore_ascii_case(&hash)) {
        return Err(Error::named("PatchedHashMismatch"));
    }
    let transaction = store.transaction(&hash)?;
    transaction.import_from(root, &patched, size)?;
    transaction.materialize_at(root, &patched, size)?;
    root.atomic_replace(
        &format!("{support}/state.json"),
        format!("{{\"patched_sha1\":\"{hash}\",\"patched_size\":{size}}}\n").as_bytes(),
    )?;
    Ok(Summary {
        status: Status::Patched,
        processors_run: applicable.len(),
        patched_sha1: hash,
        patched_size: size,
    })
}
pub fn main_class(jar: &[u8]) -> Result<String, Error> {
    let manifest = zip::read_entry(jar, "META-INF/MANIFEST.MF", 64 * 1024)
        .map_err(policy)?
        .ok_or_else(|| Error::named("ProcessorManifestMissing"))?;
    let mut found = false;
    let mut value = Vec::new();
    for raw in manifest.split(|b| *b == b'\n') {
        let line = raw.trim_ascii_end_cr();
        if found && line.starts_with(b" ") {
            value.extend_from_slice(&line[1..]);
            continue;
        }
        if found {
            break;
        }
        if let Some(rest) = line.strip_prefix(b"Main-Class:") {
            value.extend_from_slice(trim_space_tab(rest));
            found = true;
        }
    }
    if !found {
        return Err(Error::named("ProcessorMainClassMissing"));
    }
    if value.is_empty()
        || value.len() > 512
        || !value
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || b"._$".contains(c))
    {
        return Err(Error::named("InvalidProcessorMainClass"));
    }
    Ok(String::from_utf8(value).unwrap())
}
fn trim_space_tab(mut s: &[u8]) -> &[u8] {
    while s.first().is_some_and(|c| b" \t".contains(c)) {
        s = &s[1..]
    }
    while s.last().is_some_and(|c| b" \t".contains(c)) {
        s = &s[..s.len() - 1]
    }
    s
}
trait TrimCr {
    fn trim_ascii_end_cr(&self) -> &[u8];
}
impl TrimCr for [u8] {
    fn trim_ascii_end_cr(&self) -> &[u8] {
        let mut s = self;
        while s.ends_with(b"\r") {
            s = &s[..s.len() - 1]
        }
        s
    }
}

// Same one-pass placeholder grammar as version::neoforge::substitute, retaining
// native path bytes / Windows wide units instead of lossy Unicode conversion.
fn substitute_os(
    template: &str,
    map: &HashMap<String, OsString>,
    libraries: &Path,
) -> Result<OsString, Error> {
    if template.is_empty() || template.len() > 4096 {
        return Err(Error::named("InvalidProcessorArgument"));
    }
    if template.starts_with('[') && template.ends_with(']') {
        return Ok(libraries
            .join(neo::artifact_path(&template[1..template.len() - 1]).map_err(policy)?)
            .into_os_string());
    }
    let mut rest = template;
    let mut out = OsString::new();
    while let Some(start) = rest.find('{') {
        out.push(&rest[..start]);
        let end = rest[start + 1..]
            .find('}')
            .map(|i| i + start + 1)
            .ok_or_else(|| Error::named("UnresolvedPlaceholder"))?;
        let key = &rest[start + 1..end];
        if key.is_empty()
            || key.len() > 64
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err(Error::named("InvalidPlaceholderName"));
        }
        out.push(
            map.get(key)
                .ok_or_else(|| Error::named("UnresolvedPlaceholder"))?,
        );
        rest = &rest[end + 1..];
    }
    out.push(rest);
    if out.as_encoded_bytes().iter().any(|b| b"{}".contains(b)) {
        return Err(Error::named("UnresolvedPlaceholder"));
    }
    Ok(out)
}
