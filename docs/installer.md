# Download Executor

`install execute` regenerates a trusted install plan and downloads its `files`
into a caller-selected launcher data root.

```sh
jmcl-core install execute 1.21.11 --directory ./.minecraft \
  --store-directory ./.jmclcore --workers 8
```

`--source bmclapi` prefers BMCLAPI for static files. If mirror metadata or its
asset index cannot be resolved, planning retries with Mojang metadata while
retaining BMCLAPI-first file URLs and Mojang fallback URLs.

## Pipeline

For each planned file, the executor:

1. Opens an existing regular file without following its final symlink.
2. Accepts the cache entry only when size and SHA-1 both match.
3. Imports a valid existing instance file into the shared store when possible.
4. Otherwise streams the object into a store-local temporary file.
5. Rejects responses larger or smaller than the declared size.
6. Verifies SHA-1 before publication.
7. Materializes the instance path with a hard link or atomic copy fallback.

Temporary files are cleaned on ordinary errors. A process crash can still leave
implementation-generated temporary names on platforms that cannot use unnamed
files.

## Concurrency

The executor accepts 1 through 32 workers and defaults to 8. Every worker owns
an independent HTTP client, file, hasher, and bounded writer. Object lock files
coordinate identical artifacts across workers and processes. `store gc` uses
the same one-digest-at-a-time lock before removing an unlinked object, so an
active installer and collector cannot race publication or materialization.

After the first final failure, no new jobs are assigned. Active jobs finish
their cleanup before the plan and HTTP client are released.

## Retries And Mirrors

`--retries 2` means at most three attempts per file. With BMCLAPI, earlier
attempts use the mirror URL and the final attempt uses the Mojang URL stored in
`fallback_url`. Official-source plans retry the official URL.

Current retries use a short increasing delay and apply to transport, HTTP,
length, and hash failures. A later policy can narrow retries based on HTTP
status and expose configurable backoff.

## Current Scope

A successful `install execute` result means every static file in the plan is
present and verified. Runtime preparation is a separate `install prepare`
operation. Download execution itself does not perform:

- Persisting a version JSON separate from the generated plan.
- Request cancellation or resumption of partial temporary files.

Those operations remain explicit later stages so an interrupted download never
publishes partially prepared runtime state.

See `docs/preparer.md` for native extraction and asset projection.
