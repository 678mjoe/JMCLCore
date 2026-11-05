//! Pure top-level native argv diagnostics, before String conversion.
use std::{
    ffi::OsString,
    io::{self, Write},
};
const NESTED_GROUPS: &[(&str, &str, &[&str], &str)] = &[
    (
        "instance",
        "mods",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core instance mods <list|install|set-version|enable|disable|remove|adopt> ID [options]",
    ),
    (
        "instance",
        "resourcepacks",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core instance resourcepacks <list|install|set-version|enable|disable|remove|adopt> ID [options]",
    ),
    (
        "instance",
        "shaderpacks",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core instance shaderpacks <list|install|set-version|enable|disable|remove|adopt> ID [options]",
    ),
    (
        "instance",
        "datapacks",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core instance datapacks <list|install|set-version|enable|disable|remove|adopt> ID --world NAME [options]",
    ),
    (
        "instance",
        "modpack",
        &["install"],
        "usage: jmcl-core instance modpack <install> ID [options]",
    ),
    (
        "instance",
        "worlds",
        &[
            "list",
            "show",
            "rename",
            "duplicate",
            "delete",
            "export",
            "import",
            "backup",
            "backups",
            "restore",
            "backups-delete",
        ],
        "usage: jmcl-core instance worlds <list|show|rename|duplicate|delete|export|import|backup|backups|restore|backups-delete> ID [ARGS] [--instances-directory PATH] [--json]",
    ),
    (
        "server",
        "worlds",
        &[
            "list",
            "show",
            "rename",
            "delete",
            "backup",
            "backups",
            "restore",
            "backups-delete",
        ],
        "usage: jmcl-core server worlds <list|show|rename|delete|backup|backups|restore|backups-delete> ID [ARGS] [--servers-directory PATH] [--json]",
    ),
    (
        "server",
        "mods",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core server mods <list|install|set-version|enable|disable|remove|adopt> ID [options]",
    ),
    (
        "server",
        "datapacks",
        &[
            "list",
            "install",
            "set-version",
            "enable",
            "disable",
            "remove",
            "adopt",
        ],
        "usage: jmcl-core server datapacks <list|install|set-version|enable|disable|remove|adopt> ID [options]",
    ),
];
pub fn diagnostics(
    args: &[OsString],
    out: &mut impl Write,
    err: &mut impl Write,
) -> Option<io::Result<u8>> {
    if !args.iter().any(|v| v.to_str().is_none()) {
        return None;
    }
    for (top, group, methods, usage) in NESTED_GROUPS {
        if args.first().and_then(|v| v.to_str()) == Some(*top)
            && args.get(1).and_then(|v| v.to_str()) == Some(*group)
            && !args
                .get(2)
                .and_then(|v| v.to_str())
                .is_some_and(|method| methods.contains(&method))
        {
            if *top == "server"
                && matches!(*group, "mods" | "datapacks")
                && !cfg!(target_os = "linux")
            {
                return Some(
                    writeln!(
                        err,
                        "Server management is currently supported only on Linux"
                    )
                    .map(|_| 1),
                );
            }
            return Some(writeln!(err, "{usage}").map(|_| 2));
        }
    }
    let command = args.first()?;
    match command.to_str() {
        Some("help" | "--help") => Some(out.write_all(include_bytes!("help.txt")).map(|_| 0)),
        Some("instance") if !matches!(args.get(1).and_then(|v| v.to_str()), Some("create" | "list" | "show" | "delete" | "install" | "launch" | "mods" | "resourcepacks" | "shaderpacks" | "datapacks" | "modpack" | "worlds")) => {
            Some(err.write_all(b"usage: jmcl-core instance <create|list|show|delete|install|launch|mods|resourcepacks|shaderpacks|datapacks|modpack|worlds> [options]\n").map(|_| 2))
        }
        Some("install") if !matches!(args.get(1).and_then(|v| v.to_str()), Some("plan" | "execute" | "prepare")) => {
            Some(err.write_all(b"usage: jmcl-core install <plan|execute|prepare> ID [options]\n").map(|_| 2))
        }
        Some("launch") if args.get(1).and_then(|v| v.to_str()) != Some("execute") => {
            Some(err.write_all(b"usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [options]\n").map(|_| 2))
        }
        Some("store") if args.get(1).and_then(|v| v.to_str()) != Some("gc") => {
            Some(err.write_all(b"usage: jmcl-core store gc [--store-directory PATH] [--min-age-seconds N] [--execute] [--json]\n").map(|_| 2))
        }
        Some("server") if !matches!(args.get(1).and_then(|v| v.to_str()), Some("create" | "list" | "show" | "delete" | "install" | "properties" | "worlds" | "mods" | "datapacks" | "start" | "stop" | "restart" | "status" | "logs" | "command" | "rcon" | "query")) => {
            Some(err.write_all(b"usage: jmcl-core server <create|list|show|delete|install|properties|worlds|mods|datapacks|start|stop|restart|status|logs|command|rcon|query> [options]\n").map(|_| 2))
        }
        Some("server") if args.get(1).and_then(|v| v.to_str()) == Some("properties")
            && !matches!(args.get(2).and_then(|v| v.to_str()), Some("get" | "set")) => {
            Some(err.write_all(b"usage: jmcl-core server properties <get|set> ID [options]\n").map(|_| 2))
        }
        Some("server") if args.get(1).and_then(|v| v.to_str()) == Some("rcon")
            && args.get(2).and_then(|v| v.to_str()) != Some("command") => {
            Some(err.write_all(b"usage: jmcl-core server rcon command ID --command TEXT --password-file PATH [--servers-directory PATH] [--max-bytes N]\n").map(|_| 2))
        }
        Some("rpc") => Some(err.write_all(b"usage: jmcl-core rpc\n").map(|_| 2)),
        Some("version")
            if !matches!(args.get(1).and_then(|v| v.to_str()), Some("list" | "show")) =>
        {
            Some(
                err.write_all(b"usage: jmcl-core version [--json]\n")
                    .map(|_| 2),
            )
        }
        Some("version" | "install") => crate::version::native_invalid_usage(args)
            .map(|usage| writeln!(err, "{usage}").map(|_| 2)),
        Some(
            "store"
            | "java"
            | "instance"
            | "server"
            | "launch"
            | "__server_bootstrap"
            | "__server_supervisor",
        ) => None,
        _ => Some((|| {
            err.write_all(b"unknown command: ")?;
            err.write_all(&crate::native_bytes::os_bytes(command))?;
            err.write_all(b"\nrun `jmcl-core --help` for usage\n")?;
            Ok(2)
        })()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw() -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![255])
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xd800])
        }
    }
    #[test]
    fn unknown_command_bytes_are_not_replaced() {
        let raw = raw();
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            diagnostics(std::slice::from_ref(&raw), &mut out, &mut err)
                .unwrap()
                .unwrap(),
            2
        );
        let mut expected = b"unknown command: ".to_vec();
        expected.extend(crate::native_bytes::os_bytes(&raw));
        expected.extend(b"\nrun `jmcl-core --help` for usage\n");
        assert_eq!(err, expected);
        assert!(out.is_empty());
    }
    #[test]
    fn help_and_usage_are_pure_even_with_raw_tail() {
        for (command, code, stdout, stderr) in [
            (
                "help",
                0,
                include_bytes!("help.txt").as_slice(),
                b"".as_slice(),
            ),
            (
                "--help",
                0,
                include_bytes!("help.txt").as_slice(),
                b"".as_slice(),
            ),
            (
                "rpc",
                2,
                b"".as_slice(),
                b"usage: jmcl-core rpc\n".as_slice(),
            ),
            (
                "version",
                2,
                b"".as_slice(),
                b"usage: jmcl-core version [--json]\n".as_slice(),
            ),
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                diagnostics(&[command.into(), raw()], &mut out, &mut err)
                    .unwrap()
                    .unwrap(),
                code
            );
            assert_eq!(out, stdout);
            assert_eq!(err, stderr);
        }
    }
    #[test]
    fn known_operational_routes_and_utf8_are_not_claimed() {
        for command in ["java", "__server_bootstrap", "__server_supervisor"] {
            assert!(
                diagnostics(&[command.into(), raw()], &mut Vec::new(), &mut Vec::new()).is_none()
            );
        }
        for (command, methods) in [
            ("install", vec!["plan", "execute", "prepare"]),
            ("launch", vec!["execute"]),
            ("store", vec!["gc"]),
            (
                "server",
                vec![
                    "create",
                    "list",
                    "show",
                    "delete",
                    "install",
                    "properties",
                    "worlds",
                    "mods",
                    "datapacks",
                    "start",
                    "stop",
                    "restart",
                    "status",
                    "logs",
                    "command",
                    "rcon",
                    "query",
                ],
            ),
            (
                "instance",
                vec![
                    "create",
                    "list",
                    "show",
                    "delete",
                    "install",
                    "launch",
                    "mods",
                    "resourcepacks",
                    "shaderpacks",
                    "datapacks",
                    "modpack",
                    "worlds",
                ],
            ),
        ] {
            for method in methods {
                if (command == "server" && matches!(method, "properties" | "rcon"))
                    || NESTED_GROUPS
                        .iter()
                        .any(|(top, group, _, _)| *top == command && *group == method)
                {
                    continue; // The nested known-method test below protects these routes.
                }
                assert!(
                    diagnostics(
                        &[command.into(), method.into(), raw()],
                        &mut Vec::new(),
                        &mut Vec::new()
                    )
                    .is_none()
                );
            }
        }
        assert!(
            diagnostics(
                &[
                    "version".into(),
                    "show".into(),
                    raw(),
                    "--source".into(),
                    "official".into()
                ],
                &mut Vec::new(),
                &mut Vec::new(),
            )
            .is_none()
        );
        assert!(diagnostics(&["--help".into()], &mut Vec::new(), &mut Vec::new()).is_none());
    }
    #[test]
    fn nested_content_world_usage_never_invokes_known_operations() {
        for (top, group, methods, usage) in NESTED_GROUPS {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let unsupported = *top == "server"
                && matches!(*group, "mods" | "datapacks")
                && !cfg!(target_os = "linux");
            assert_eq!(
                diagnostics(&[(*top).into(), (*group).into(), raw()], &mut out, &mut err)
                    .unwrap()
                    .unwrap(),
                if unsupported { 1 } else { 2 }
            );
            assert!(out.is_empty());
            let expected = if unsupported {
                "Server management is currently supported only on Linux\n".to_owned()
            } else {
                format!("{usage}\n")
            };
            assert_eq!(err, expected.as_bytes());
            for method in *methods {
                assert!(
                    diagnostics(
                        &[(*top).into(), (*group).into(), (*method).into(), raw()],
                        &mut Vec::new(),
                        &mut Vec::new()
                    )
                    .is_none()
                );
            }
        }
    }
    #[test]
    fn nested_server_unknown_diagnostics_yield_to_known_methods() {
        for (group, methods, expected) in [
            (
                "properties",
                vec!["get", "set"],
                "usage: jmcl-core server properties <get|set> ID [options]\n",
            ),
            (
                "rcon",
                vec!["command"],
                "usage: jmcl-core server rcon command ID --command TEXT --password-file PATH [--servers-directory PATH] [--max-bytes N]\n",
            ),
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                diagnostics(&["server".into(), group.into(), raw()], &mut out, &mut err)
                    .unwrap()
                    .unwrap(),
                2
            );
            assert!(out.is_empty());
            assert_eq!(err, expected.as_bytes());
            for method in methods {
                assert!(
                    diagnostics(
                        &["server".into(), group.into(), method.into(), raw()],
                        &mut Vec::new(),
                        &mut Vec::new()
                    )
                    .is_none()
                );
            }
        }
    }
    #[test]
    fn native_plan_diagnostics_preserve_opaque_loader_values() {
        for option in ["--fabric-loader", "--neoforge-version", "--forge-version"] {
            let mut args = vec!["install".into(), "plan".into(), raw(), option.into(), raw()];
            assert!(diagnostics(&args, &mut Vec::new(), &mut Vec::new()).is_none());
            args.extend([
                "--source".into(),
                "bad".into(),
                "--source".into(),
                "official".into(),
            ]);
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(diagnostics(&args, &mut out, &mut err).unwrap().unwrap(), 2);
            assert!(out.is_empty());
            assert!(err.starts_with(b"usage: jmcl-core install plan ID "));
        }
        let args = vec![
            "install".into(),
            "plan".into(),
            raw(),
            "--fabric-loader".into(),
            "--json".into(),
        ];
        assert!(diagnostics(&args, &mut Vec::new(), &mut Vec::new()).is_none());
    }
    #[test]
    fn invalid_native_version_syntax_never_reaches_transport() {
        for (tail, expected) in [
            (
                vec!["list".into(), raw()],
                "usage: jmcl-core version list [--type TYPE] [--limit N] [--source SOURCE] [--json]\n",
            ),
            (
                vec!["list".into(), "--source".into(), raw()],
                "usage: jmcl-core version list [--type TYPE] [--limit N] [--source SOURCE] [--json]\n",
            ),
            (
                vec!["show".into(), raw(), "--source".into()],
                "usage: jmcl-core version show ID [--source SOURCE] [--json]\n",
            ),
            (
                vec!["show".into(), "".into(), raw()],
                "usage: jmcl-core version show ID [--source SOURCE] [--json]\n",
            ),
            (
                vec![
                    "show".into(),
                    raw(),
                    "--source".into(),
                    "bad".into(),
                    "--source".into(),
                    "official".into(),
                ],
                "usage: jmcl-core version show ID [--source SOURCE] [--json]\n",
            ),
        ] {
            let mut args = vec!["version".into()];
            args.extend(tail);
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(diagnostics(&args, &mut out, &mut err).unwrap().unwrap(), 2);
            assert!(out.is_empty());
            assert_eq!(err, expected.as_bytes());
        }
        for tail in [
            vec!["show".into(), raw()],
            vec!["show".into(), raw(), "--json".into()],
        ] {
            let mut args = vec!["version".into()];
            args.extend(tail);
            assert!(diagnostics(&args, &mut Vec::new(), &mut Vec::new()).is_none());
        }
    }
    #[test]
    fn unknown_raw_group_subcommands_are_pure_usage() {
        for (command, expected) in [
            (
                "store",
                "usage: jmcl-core store gc [--store-directory PATH] [--min-age-seconds N] [--execute] [--json]\n",
            ),
            (
                "server",
                "usage: jmcl-core server <create|list|show|delete|install|properties|worlds|mods|datapacks|start|stop|restart|status|logs|command|rcon|query> [options]\n",
            ),
            (
                "instance",
                "usage: jmcl-core instance <create|list|show|delete|install|launch|mods|resourcepacks|shaderpacks|datapacks|modpack|worlds> [options]\n",
            ),
            (
                "install",
                "usage: jmcl-core install <plan|execute|prepare> ID [options]\n",
            ),
            (
                "launch",
                "usage: jmcl-core launch execute ID [--directory PATH] --offline-name NAME [options]\n",
            ),
        ] {
            let mut out = Vec::new();
            let mut err = Vec::new();
            assert_eq!(
                diagnostics(&[command.into(), raw()], &mut out, &mut err)
                    .unwrap()
                    .unwrap(),
                2
            );
            assert!(out.is_empty());
            assert_eq!(err, expected.as_bytes());
        }
    }
}
