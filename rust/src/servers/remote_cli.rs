use super::{query, rcon, remote};
use std::{
    fs::File,
    io::{self, Read, Write},
};
use zeroize::Zeroizing;
fn usage(rcon: bool, err: &mut impl Write) -> io::Result<u8> {
    writeln!(
        err,
        "{}",
        if rcon {
            "usage: jmcl-core server rcon command ID --command TEXT --password-file PATH [--servers-directory PATH] [--max-bytes N]"
        } else {
            "usage: jmcl-core server query ID [--mode basic|full] [--servers-directory PATH] [--json]"
        }
    )?;
    Ok(2)
}
fn failure(name: &str, err: &mut impl Write) -> io::Result<u8> {
    let text = match name {
        "ServerNotRunning" => "the server is not running",
        "ServerRconDisabled" => "RCON is not enabled for this server",
        "ServerRconAuthenticationFailed" => "RCON authentication failed",
        "ServerRconUnavailable" => "RCON is unavailable",
        "ServerRconProtocolError" => "RCON returned an invalid response",
        "ServerQueryDisabled" => "Query is not enabled for this server",
        "ServerQueryUnavailable" => "Query is unavailable",
        "ServerQueryProtocolError" => "Query returned an invalid response",
        "ServerPropertiesInvalid" => "server properties are invalid",
        "InvalidServerRemoteInput" => "server RCON command or password is invalid",
        _ => "server remote operation failed",
    };
    writeln!(err, "{text}")?;
    Ok(1)
}
fn password_file(path: &str) -> io::Result<Zeroizing<Vec<u8>>> {
    #[cfg(unix)]
    let mut file = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        File::options()
            .read(true)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?
    };
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(io::Error::other("InvalidPasswordFile"));
        }
    }
    if !metadata.is_file() || metadata.len() > 4097 {
        return Err(io::Error::other("InvalidPasswordFile"));
    }
    let mut bytes = Zeroizing::new(vec![0; metadata.len() as usize]);
    file.read_exact(&mut bytes)?;
    let mut extra = Zeroizing::new([0; 1]);
    if file.read(&mut extra[..])? != 0 {
        return Err(io::Error::other("InvalidPasswordFile"));
    }
    let trim = if bytes.ends_with(b"\r\n") {
        2
    } else if bytes.ends_with(b"\n") {
        1
    } else {
        0
    };
    let length = bytes.len() - trim;
    if length == 0
        || length > 4095
        || bytes[..length]
            .iter()
            .any(|b| matches!(b, b'\r' | b'\n' | 0))
        || std::str::from_utf8(&bytes[..length]).is_err()
    {
        return Err(io::Error::other("InvalidPasswordFile"));
    }
    bytes.truncate(length);
    Ok(bytes)
}
pub fn cli(
    kind: &str,
    args: &[String],
    out: &mut impl Write,
    err: &mut impl Write,
) -> io::Result<u8> {
    let is_rcon = kind == "rcon";
    let args = if is_rcon {
        if args.first().map(String::as_str) != Some("command") {
            return usage(true, err);
        }
        &args[1..]
    } else {
        args
    };
    let Some(id) = args.first().filter(|s| !s.is_empty()) else {
        return usage(is_rcon, err);
    };
    if crate::instances::validate_id(id).is_err() {
        return usage(is_rcon, err);
    }
    let mut directory = "servers";
    let mut password = None;
    let mut command = None;
    let mut limit = rcon::MAX_REPLY;
    let mut mode = query::Mode::Full;
    let mut json = false;
    let mut i = 1;
    while i < args.len() {
        let arg = args[i].as_str();
        if !is_rcon && arg == "--json" {
            json = true;
            i += 1;
            continue;
        }
        let Some(value) = args.get(i + 1).filter(|s| !s.is_empty()) else {
            return usage(is_rcon, err);
        };
        match arg {
            "--servers-directory" => directory = value,
            "--command" if is_rcon => command = Some(value.as_str()),
            "--password-file" if is_rcon => password = Some(value.as_str()),
            "--max-bytes" if is_rcon => {
                let Ok(n) = value.parse::<usize>() else {
                    return usage(true, err);
                };
                limit = n;
            }
            "--mode" if !is_rcon => {
                mode = match value.as_str() {
                    "basic" => query::Mode::Basic,
                    "full" => query::Mode::Full,
                    _ => return usage(false, err),
                }
            }
            _ => return usage(is_rcon, err),
        }
        i += 2;
    }
    if is_rcon {
        let (Some(password), Some(command)) = (password, command) else {
            return usage(true, err);
        };
        if limit == 0
            || limit > rcon::MAX_REPLY
            || command.is_empty()
            || command.len() > 4095
            || command.contains('\0')
        {
            return usage(true, err);
        }
        let password = match password_file(password) {
            Ok(p) => p,
            Err(_) => {
                writeln!(err, "password file is invalid")?;
                return Ok(2);
            }
        };
        let reply = match remote::execute_rcon(
            directory,
            id,
            std::str::from_utf8(&password).unwrap(),
            command,
            limit,
        ) {
            Ok(r) => r,
            Err(e) => return failure(e, err),
        };
        out.write_all(reply.text.as_bytes())?;
        if reply.truncated {
            writeln!(err, "RCON reply truncated at requested limit")?;
        }
        Ok(0)
    } else {
        let r = match remote::execute_query(directory, id, mode) {
            Ok(r) => r,
            Err(e) => return failure(e, err),
        };
        if json {
            serde_json::to_writer(&mut *out, &remote::query_value(&r))?;
            writeln!(out)?;
        } else {
            writeln!(
                out,
                "hostname\t{}\ngame_type\t{}\nmap\t{}\nversion\t{}\nplayers\t{}/{}\nhost\t{}:{}",
                r.hostname,
                r.game_type,
                r.map,
                r.version.as_deref().unwrap_or(""),
                r.players,
                r.max_players,
                r.host_ip,
                r.host_port
            )?;
            if let Some(plugins) = r.plugins {
                writeln!(out, "plugins\t{plugins}")?;
            }
            for player in r.player_names {
                writeln!(out, "player\t{player}")?;
            }
        }
        Ok(0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_contents_are_single_bounded_utf8_line() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("password");
        for valid in [b"secret\n".as_slice(), b"secret\r\n", b"secret"] {
            std::fs::write(&p, valid).unwrap();
            assert_eq!(
                password_file(p.to_str().unwrap()).unwrap().as_slice(),
                b"secret"
            );
        }
        for bad in [b"".as_slice(), b"one\ntwo\n", b"x\r", b"\xff", b"\0"] {
            std::fs::write(&p, bad).unwrap();
            assert!(password_file(p.to_str().unwrap()).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn password_final_alias_rejected() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("real"), b"fixture").unwrap();
        std::os::unix::fs::symlink("real", t.path().join("alias")).unwrap();
        assert!(password_file(t.path().join("alias").to_str().unwrap()).is_err());
    }
}
