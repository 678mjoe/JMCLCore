# Planning host target before loader conflict

Pinned Zig 5a4310de protocol/install.zig:57–78 validates the complete request,
then rejects an empty ID, resolves Target.host even when both overrides exist,
applies overrides, and calls createInstallPlanForSelection. Loader conflict
occurs inside resolveMetadata (service.zig:1230). CLI install.zig:470–492 similarly
parses options before host, overrides and selection planning.

The recovered Rust loader-backed RPC and CLI checked conflict before host.
A production-used extracted preflight reproduced Conflict with zero injected
host calls even when the injected host would fail. Moving the conflict check
below host/overrides makes host failure win; malformed schema and empty IDs
still prevent host invocation. Successful host plus multiple selections still
returns the existing LoaderConflict mapping. No resolver is called by these tests.

Both legacy planning routes now use the shared preflight. The conditional
vanilla route already orders host correctly and remains unchanged. Public
request types, HTTP construction timing, loader/source policy, resolver methods,
String plans, installation and Java paths remain unchanged. The helper performs
only canonical request validation, host-result selection and loader counting.
This closes the recorded host/conflict mismatch, not loader opaque-ID support
or all error-order parity. Historical records of the earlier gap remain evidence.

## Next source-only native CLI gap

main.rs collects args_os, then offers content/local, world/read, and version/read
adapters. version_read.rs::native_request claims only `version show`, not
`install plan`. Unclaimed arguments are collected through std::env::args;
a non-UTF8 planning ID therefore cannot reach cli_params and can trigger its
Unicode conversion panic. Ordinary UTF-8 `install plan` uses the reviewed read
route, but injected byte-ID formatter tests do not establish native argv support.
The next separate review should consider a narrow no-loader `install plan` raw
adapter, preserving exact option validation/loader exclusions before transport.
No broad main rewrite or native adapter implementation is included here.
