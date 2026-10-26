// Included in the Linux facade module: CLI and RPC use the same anchored target
// and provider execution, but preserve their distinct validation/error ordering.
fn cli_usage(family: &str, op: Option<&str>) -> String {
    let Some(op) = op else {
        return format!(
            "usage: jmcl-core server {family} <list|install|set-version|enable|disable|remove|adopt> ID [options]"
        );
    };
    let tail = match op {
        "list" => "ID [--servers-directory PATH] [--json]",
        "install" => {
            "ID --project SLUG [--version ID] [--provider modrinth|curseforge] [--api-key KEY] [--no-dependencies] [--servers-directory PATH] [--store-directory PATH] [--workers N] [--retries N]"
        }
        "set-version" => {
            "ID --project REF --version ID [--api-key KEY] [--servers-directory PATH] [--store-directory PATH] [--workers N] [--retries N]"
        }
        "adopt" => "ID [--servers-directory PATH] [--store-directory PATH]",
        _ => "ID (--project REF | --file PATH) [--servers-directory PATH]",
    };
    format!("usage: jmcl-core server {family} {op} {tail}")
}
fn cli_error(out: &mut impl Write, name: &str, family: &str, op: &str) -> io::Result<u8> {
    let dp = family == "datapacks";
    if family == "mods" && op == "list" {
        if name == "ServerNotFound" {
            writeln!(out, "no server matches the given id")?
        } else {
            writeln!(out, "failed to list server mods: {name}")?
        }
        return Ok(1);
    }
    let message = match name {
        "ServerNotFound" => "no server matches the given id",
        "ServerAlreadyRunning" => {
            if dp {
                "stop the server before changing its datapacks"
            } else {
                "stop the server before changing its mods"
            }
        }
        "ServerStatusFailed" => "failed to validate the server lifecycle status",
        "InvalidServerProperties" if dp => {
            "server.properties is missing or has no usable level-name (install the server first)"
        }
        "ModLoaderRequired" if !dp => {
            "the server has no mod loader; mods require Fabric, NeoForge, or Forge"
        }
        "NoCompatibleContentVersion" => {
            if dp {
                "no release of the project supports the server's Minecraft version"
            } else {
                "no release of the project supports the server's Minecraft version and loader"
            }
        }
        "IncompatibleContentVersion" => {
            if dp {
                "the requested version does not support the server's Minecraft version"
            } else {
                "the requested version does not support the server's Minecraft version and loader"
            }
        }
        "ContentNotFound" => {
            if dp {
                "no registered datapack matches the given selector"
            } else {
                "no registered mod matches the given selector"
            }
        }
        "ModrinthVersionProjectMismatch" | "CurseforgeFileProjectMismatch" => {
            "the requested version does not belong to the registered project"
        }
        "ContentFileConflict" => {
            "a different registered content entry already occupies the target file"
        }
        "ContentKindMismatch" => {
            if dp {
                "the project is not a datapack project"
            } else {
                "the project is not a mod project"
            }
        }
        "UnsupportedProjectType" => "the project type is not supported",
        "CurseforgeApiKeyRequired" => {
            if dp {
                "switching a curseforge datapack requires --api-key (supplied by the embedding application)"
            } else {
                "switching a curseforge mod requires --api-key (supplied by the embedding application)"
            }
        }
        "CurseforgeDownloadUnavailable" => {
            "the project file is not available for third-party downloads"
        }
        "InvalidCurseforgeId" => "curseforge projects and files use numeric ids",
        "UnsuccessfulHttpStatus" => {
            "the content API returned an unsuccessful HTTP status (check the api key)"
        }
        _ => {
            if dp {
                writeln!(out, "server datapacks {op} failed: {name}")?
            } else {
                writeln!(out, "server content operation failed: {name}")?
            }
            return Ok(1);
        }
    };
    writeln!(out, "{message}")?;
    Ok(1)
}
fn cli_params(args: &[String], op: &str) -> Result<(Params, bool), ()> {
    let id = args.first().ok_or(())?;
    crate::instances::validate_id(id).map_err(|_| ())?;
    let mut p = Params {
        directory: "servers".into(),
        id: id.clone(),
        project: None,
        local_project: None,
        file: None,
        version: None,
        provider: None,
        key: None,
        store: crate::store::DEFAULT_DIRECTORY.into(),
        dependencies: true,
        workers: 8,
        retries: 2,
    };
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--json" && op == "list" {
            json = true;
            i += 1;
            continue;
        }
        if a == "--no-dependencies" && op == "install" {
            p.dependencies = false;
            i += 1;
            continue;
        }
        let v = args.get(i + 1).ok_or(())?;
        if v.is_empty() && op != "list" {
            return Err(());
        }
        match a {
            "--servers-directory" => p.directory = v.clone(),
            "--store-directory" if matches!(op, "install" | "set-version" | "adopt") => {
                p.store = v.clone()
            }
            "--project"
                if matches!(
                    op,
                    "install" | "set-version" | "enable" | "disable" | "remove"
                ) =>
            {
                p.project = Some(v.clone())
            }
            "--file" if matches!(op, "enable" | "disable" | "remove") => p.file = Some(v.as_bytes().to_vec()),
            "--version" if matches!(op, "install" | "set-version") => p.version = Some(v.clone()),
            "--provider" if op == "install" => {
                if !matches!(v.as_str(), "modrinth" | "curseforge") {
                    return Err(());
                }
                p.provider = Some(v.clone());
            }
            "--api-key" if matches!(op, "install" | "set-version") => {
                p.key = Some(Zeroizing::new(v.clone()))
            }
            "--workers" | "--retries" if matches!(op, "install" | "set-version") => {
                let n = cli_number(v).ok_or(())?;
                if a == "--workers" {
                    if n == 0 || n > 32 {
                        return Err(());
                    }
                    p.workers = n;
                } else {
                    if n > 5 {
                        return Err(());
                    }
                    p.retries = n;
                }
            }
            _ => return Err(()),
        }
        i += 2;
    }
    Ok((p, json))
}
pub(super) fn cli_run<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(
    family: &str,
    args: &[String],
    out: &mut impl Write,
    errors: &mut impl Write,
    metadata: &M,
    transport: &T,
) -> io::Result<u8> {
    let Some(op) = args.first().map(String::as_str).filter(|s| {
        matches!(
            *s,
            "list" | "install" | "set-version" | "enable" | "disable" | "remove" | "adopt"
        )
    }) else {
        writeln!(errors, "{}", cli_usage(family, None))?;
        return Ok(2);
    };
    if args.len() == 1 {
        writeln!(errors, "{}", cli_usage(family, Some(op)))?;
        return Ok(2);
    }
    let (p, json_output) = match cli_params(&args[1..], op) {
        Ok(p) => p,
        Err(()) => {
            writeln!(errors, "error: InvalidArguments")?;
            return Ok(1);
        }
    };
    cli_execute(family,op,(p,json_output),out,errors,metadata,transport)
}
// Only local operations discard the display name; preserve real operational
// fields in this tuple without changing the network/adoption target helper.
fn cli_read_target(p: &Params) -> Result<(File, String, Option<&'static str>), String> {
    cli_read_target_native(std::ffi::OsStr::new(&p.directory), &p.id)
}
fn cli_read_target_native(directory: &std::ffi::OsStr, id: &str) -> Result<(File, String, Option<&'static str>), String> {
    let root = store::open_root_native(directory).map_err(|_| "ServerNotFound")?;
    let m = store::load_read(&root, id).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound || e.to_string() == "InvalidServerId" {
            "ServerNotFound".to_owned()
        } else {
            e.to_string()
        }
    })?;
    let loader = if m.fabric_loader.is_some() {
        Some("fabric")
    } else if m.neoforge_version.is_some() {
        Some("neoforge")
    } else if m.forge_version.is_some() {
        Some("forge")
    } else {
        None
    };
    let dir = store::open_server(&root, id).map_err(|_| "ServerNotFound")?;
    Ok((dir, m.version_id, loader))
}
fn cli_execute<M: remote::Metadata + curseforge::Metadata + Sync, T: install::Transport>(family:&str,op:&str,parsed:(Params,bool),out:&mut impl Write,errors:&mut impl Write,metadata:&M,transport:&T)->io::Result<u8>{
    let (p,json_output)=parsed;
    if op == "install" && p.project.is_none() {
        writeln!(errors, "server {family} install requires --project")?;
        return Ok(2);
    }
    if op == "set-version" && (p.project.is_none() || p.version.is_none()) {
        writeln!(
            errors,
            "server {family} set-version requires --project and --version"
        )?;
        return Ok(2);
    }
    if matches!(op, "enable" | "disable" | "remove") && p.project.is_some() == p.file.is_some() {
        writeln!(
            errors,
            "server {family} {op} requires exactly one of --project or --file"
        )?;
        return Ok(2);
    }
    let provider = if p.provider.as_deref() == Some("curseforge") {
        Provider::Curseforge
    } else {
        Provider::Modrinth
    };
    if op == "install" {
        if provider == Provider::Curseforge && p.key.is_none() {
            writeln!(
                errors,
                "curseforge installs require --api-key (supplied by the embedding application)"
            )?;
            return Ok(2);
        }
        if provider == Provider::Modrinth && p.key.is_some() {
            writeln!(errors, "--api-key is only used with --provider curseforge")?;
            return Ok(2);
        }
    }
    let cas = if matches!(op, "install" | "set-version" | "adopt") {
        match Store::open(&p.store) {
            Ok(s) => Some(s),
            Err(e) => {
                writeln!(
                    errors,
                    "failed to open artifact store: {}",
                    crate::instances::cli_error_name(&e)
                )?;
                return Ok(1);
            }
        }
    } else {
        None
    };
    let prepared = (|| -> Result<_, String> {
        let (dir, game, loader) = if matches!(op, "list" | "enable" | "disable" | "remove" | "adopt") {
            cli_read_target(&p)?
        } else {
            target(&p, family == "mods" && op == "install")?
        };
        let guard = if op == "list" {
            None
        } else {
            Some(lifecycle::lock_stopped(&dir).map_err(str::to_owned)?)
        };
        let dir = if family == "datapacks" {
            world(&dir, op != "list")?
        } else {
            Some(dir)
        };
        Ok((
            dir.map(Root::from_directory),
            game,
            if family == "datapacks" { None } else { loader },
            guard,
        ))
    })();
    let (root, game, loader, _guard) = match prepared {
        Ok(p) => p,
        Err(e) => return cli_error(errors, &e, family, op),
    };
    let kind = if family == "mods" {
        Kind::Mod
    } else {
        Kind::Datapack
    };
    let fields = if matches!(op, "install" | "set-version") {
        let mut completed = 0;
        install::stream::stream(
            |reporter| {
                network(
                    &p,
                    op,
                    kind,
                    root.as_ref().unwrap(),
                    cas.as_ref().unwrap(),
                    &game,
                    loader,
                    provider,
                    metadata,
                    transport,
                    install::Options {
                        workers: p.workers,
                        retries: p.retries,
                        sync_files: true,
                    },
                    reporter,
                )
            },
            |event| {
                match event {
                    install::ByteEvent::Retry(r) => {
                        write!(errors,"retry {}/{}: ",r.attempt,r.max_attempts)?;
                        errors.write_all(&r.path.0)?;writeln!(errors," ({})",r.error_name)?;
                    },
                    install::ByteEvent::Progress(p)
                        if p.files_completed > completed
                            && (p.files_completed % 100 == 0
                                || p.files_completed == p.files_total) =>
                    {
                        completed = p.files_completed;
                        writeln!(
                            errors,
                            "download progress: {}/{} files, {}/{} bytes verified",
                            p.files_completed, p.files_total, p.bytes_verified, p.bytes_total
                        )?;
                    }
                    _ => (),
                }
                errors.flush()
            },
        )?
        .map_err(|e| e.name().to_owned())
    } else {
        local(&p, op, kind, root.as_ref(), cas.as_ref()).map_err(local_name)
    };
    cli_emit(family,op,json_output,fields,out,errors)
}
fn cli_emit(family:&str,op:&str,json_output:bool,fields:Result<Value,String>,out:&mut impl Write,errors:&mut impl Write)->io::Result<u8>{
    let v = match fields {
        Ok(v) => v,
        Err(e) => return cli_error(errors, &e, family, op),
    };
    let s = |v: &Value, n: &str| v[n].as_str().unwrap_or("-").to_owned();
    match op {
        "list" if json_output => {
            serde_json::to_writer(&mut *out, &v)?;
            writeln!(out)?;
        }
        "list" => {
            for e in v["entries"].as_array().unwrap() {
                write!(out, "{}\t", s(e, "kind"))?;
                if let Some(file) = e["file"].as_str() {
                    out.write_all(file.as_bytes())?;
                } else {
                    let bytes: Vec<u8> = e["file"].as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect();
                    out.write_all(&bytes)?;
                }
                writeln!(
                    out,
                    "\t{}\t{}\t{}\t{}\t{}\t{}",
                    s(e, "sha1"),
                    e["size"],
                    s(&e["source"], "provider"),
                    s(&e["source"], "slug"),
                    if e["enabled"] == true {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    if e["as_dependency"] == true {
                        "dependency"
                    } else {
                        "explicit"
                    }
                )?;
            }
            for u in v["unmanaged"].as_array().unwrap() {
                out.write_all(b"unmanaged\t")?;
                if let Some(s) = u.as_str() {
                    out.write_all(s.as_bytes())?;
                } else {
                    let bytes: Vec<u8> = u.as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect();
                    out.write_all(&bytes)?;
                }
                out.write_all(b"\n")?;
            }
        }
        "install" => {
            for e in v["installed"].as_array().unwrap() {
                out.write_all(b"installed\t")?;write_content_file(out,&e["file"])?;
                writeln!(out,"\t{}\t{}\t{}",s(e,"sha1"),s(e,"slug"),if e["as_dependency"]==true {"dependency"}else{"explicit"})?;
            }
            writeln!(out,"skipped\t{}",v["skipped"])?;
        }
        "set-version" => {
            if v["changed"]==true {
                out.write_all(b"changed\t")?;write_content_file(out,&v["file"])?;
                writeln!(out,"\t{}\t{}\t{}",s(&v,"sha1"),s(&v,"old_version_id"),s(&v,"new_version_id"))?;
            }else{
                out.write_all(b"unchanged\t")?;write_content_file(out,&v["file"])?;writeln!(out,"\t{}",s(&v,"new_version_id"))?;
            }
        }
        "adopt" => {
            let a = v["adopted"].as_array().unwrap();
            for e in a {
                out.write_all(b"adopted\t")?;
                if let Some(file) = e["file"].as_str() { out.write_all(file.as_bytes())?; }
                else { let bytes: Vec<u8> = e["file"].as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect(); out.write_all(&bytes)?; }
                writeln!(out, "\t{}\t{}", s(e, "sha1"), e["size"])?;
            }
            writeln!(out, "adoptedCount\t{}", a.len())?;
        }
        "remove" => {
            out.write_all(b"removed\t")?;
            write_content_file(out, &v["removed"]["file"])?;
            writeln!(out,"\t{}",s(&v["removed"],"sha1"))?;
        }
        _ => {
            let state = if op == "enable" {
                "enabled"
            } else {
                "disabled"
            };
            if v["changed"] == true {
                write!(out,"{state}\t")?; write_content_file(out,&v["entry"]["file"])?; writeln!(out)?;
            } else {
                out.write_all(b"unchanged\t")?; write_content_file(out,&v["entry"]["file"])?; writeln!(out,"\t{state}")?;
            }
        }
    }
    Ok(0)
}

fn cli_number(s: &str) -> Option<u8> {
    let s = s.strip_prefix('+').unwrap_or(s);
    if s.is_empty() || s.starts_with('_') || s.ends_with('_') {
        return None;
    }
    let mut n = 0u8;
    for b in s.bytes() {
        if b == b'_' {
            continue;
        }
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(b - b'0')?;
    }
    Some(n)
}

fn write_content_file(out: &mut impl Write, file: &Value) -> io::Result<()> {
    if let Some(file)=file.as_str(){out.write_all(file.as_bytes())}
    else{let bytes:Vec<u8>=file.as_array().unwrap().iter().map(|b|b.as_u64().unwrap() as u8).collect();out.write_all(&bytes)}
}

struct LocalCliParams {
    id: String,
    directory: Vec<u8>,
    project: Option<Vec<u8>>,
    file: Option<Vec<u8>>,
    json: bool,
}
pub(super) fn cli_local_bytes(family:&str,op:&str,args:&[crate::content::LocalArg<'_>],out:&mut impl Write,errors:&mut impl Write)->io::Result<u8>{
    let parse=|| -> Result<LocalCliParams,()> {
        let id=args.first().and_then(|a|a.text()).ok_or(())?;
        crate::instances::validate_id(id).map_err(|_| ())?;
        let mut p=LocalCliParams { id:id.to_owned(),directory:b"servers".to_vec(),project:None,file:None,json:false };
        let mut i=1;
        while i<args.len(){
            let key=args[i].text().ok_or(())?;
            if key=="--json" && op=="list" { p.json=true;i+=1;continue; }
            let v=args.get(i+1).ok_or(())?;
            if v.bytes().is_empty() && op!="list" {return Err(());}
            match key {
                "--servers-directory"=>p.directory=v.bytes().to_vec(),
                "--project" if op!="list"=>p.project=Some(v.bytes().to_vec()),
                "--file" if op!="list"=>p.file=Some(v.bytes().to_vec()),
                _=>return Err(()),
            }
            i+=2;
        }
        Ok(p)
    };
    let p=match parse(){Ok(p)=>p,Err(())=>{writeln!(errors,"error: InvalidArguments")?;return Ok(1);}};
    if op!="list" && p.project.is_some()==p.file.is_some() {
        writeln!(errors,"server {family} {op} requires exactly one of --project or --file")?;
        return Ok(2);
    }
    let prepared=(|| -> Result<_,String> {
        let directory=crate::store::path_from_bytes(&p.directory).map_err(|_| "ServerNotFound")?;
        let (dir,_,_)=cli_read_target_native(&directory,&p.id)?;
        let guard=if op=="list" { None } else { Some(lifecycle::lock_stopped(&dir).map_err(str::to_owned)?) };
        let directory=if family=="datapacks" { world(&dir,op!="list")? } else {Some(dir)};
        Ok((directory.map(Root::from_directory),guard))
    })();
    let (root,_guard)=match prepared {Ok(value)=>value,Err(e)=>return cli_error(errors,&e,family,op)};
    let kind=if family=="mods" {Kind::Mod} else {Kind::Datapack};
    let fields=match root.as_ref() {
        Some(root) if op=="list"=>list_fields(Some(root),kind),
        Some(root)=>content::local_project(p.project.as_deref())
            .and_then(|project|selected_fields(root,op,kind,project,p.file.as_deref())),
        None=>Ok(json!({"entries":[],"unmanaged":[]})),
    }.map_err(local_name);
    cli_emit(family,op,p.json,fields,out,errors)
}
