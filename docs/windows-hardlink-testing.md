# Windows Hard-Link Test Plan

This checklist validates JMCL's shared artifact store, Windows hard-link
implementation, copy fallback, instance isolation, and recovery behavior.

The Windows build outputs are:

```text
zig-out/bin/jmcl-core.exe
zig-out/bin/jmcl-core.pdb
```

The PDB is only needed for crash diagnosis, but should be kept beside the
executable during testing.

## 1. Environment

Use a disposable NTFS directory and record the Windows and filesystem versions:

```powershell
winver
fsutil fsinfo volumeinfo D:
./jmcl-core.exe version
```

Define the test locations:

```powershell
$Root = "D:\JMCLTest"
$Instances = "$Root\instances"
$Store = "$Root\.jmclcore"
```

## 2. Same-Volume Reuse

Create two instances using the same Minecraft version:

```powershell
./jmcl-core.exe instance create win-a `
  --version 26.1 `
  --instances-directory $Instances

./jmcl-core.exe instance create win-b `
  --version 26.1 `
  --instances-directory $Instances
```

Install the first instance:

```powershell
./jmcl-core.exe instance install win-a `
  --instances-directory $Instances `
  --store-directory $Store `
  --workers 8
```

Expected results for a new store:

```text
downloaded    greater than 0
hardLinked    4824
copied        0
```

The exact file count may change if Mojang updates the selected version's
metadata.

Install the second instance:

```powershell
./jmcl-core.exe instance install win-b `
  --instances-directory $Instances `
  --store-directory $Store `
  --workers 8
```

Expected results:

```text
downloaded         0
cached              4824
hardLinked          4824
copied              0
transferredBytes    0
```

Metadata resolution may still use the network. `transferredBytes` only counts
static artifact downloads.

## 3. File Identity

Calculate the client JAR hash and locate its store object:

```powershell
$JarA = "$Instances\win-a\.minecraft\versions\26.1\26.1.jar"
$JarB = "$Instances\win-b\.minecraft\versions\26.1\26.1.jar"
$Hash = (Get-FileHash $JarA -Algorithm SHA1).Hash.ToLower()
$Object = "$Store\objects\sha1\$($Hash.Substring(0,2))\$Hash"

$Hash
$Object
```

Inspect the links and file IDs:

```powershell
fsutil hardlink list $Object
fsutil file queryfileid $Object
fsutil file queryfileid $JarA
fsutil file queryfileid $JarB
```

Expected behavior:

- All three paths have the same file ID.
- The hard-link list contains the store and both instance paths.
- The shared files have the read-only attribute.

```powershell
Get-Item $Object | Format-List FullName,Attributes,LinkType
Get-Item $JarA | Format-List FullName,Attributes,LinkType
Get-Item $JarB | Format-List FullName,Attributes,LinkType
```

## 4. Reinstallation

Reinstall the same instance:

```powershell
./jmcl-core.exe instance install win-a `
  --instances-directory $Instances `
  --store-directory $Store
```

Verify that:

- No `AccessDenied` or read-only replacement error occurs.
- `downloaded` is `0`.
- The hard-link relationship remains unchanged.
- No temporary `.jmcl-link-*` files remain.

JMCL detects when the destination is already the correct inode and does not
attempt to replace that read-only link.

## 5. Game Launch

Launch one instance:

```powershell
./jmcl-core.exe instance launch win-a `
  --instances-directory $Instances `
  --offline-name WindowsPlayer
```

Verify that:

- Minecraft reaches the main menu.
- The displayed user is `WindowsPlayer`.
- `logs/latest.log` is under the instance's `.minecraft` directory.
- A normal close returns exit code `0`.
- `.jmcl\runtime\natives` is empty after exit.

## 6. Instance Deletion

Delete the first instance:

```powershell
./jmcl-core.exe instance delete win-a `
  --instances-directory $Instances `
  --force
```

Check the remaining paths:

```powershell
Test-Path $JarB
Test-Path $Object
fsutil hardlink list $Object
```

The second instance and store object must remain valid, and `win-b` must still
launch successfully.

## 7. Cross-Volume Copy Fallback

Place the store and instances on different volumes:

```powershell
$Store = "C:\JMCLStore\.jmclcore"
$Instances = "D:\JMCLInstances"
```

Create and install another instance with these paths. Expected placement:

```text
hardLinked    0
copied        4824
```

The next instance should reuse the store without static network downloads, but
its files must still be copied because Windows cannot create hard links across
volumes.

## 8. Concurrent Installation

Create empty `win-c` and `win-d` instances, then install them simultaneously:

```powershell
$p1 = Start-Process ./jmcl-core.exe `
  -ArgumentList @("instance","install","win-c","--instances-directory",$Instances,"--store-directory",$Store) `
  -RedirectStandardOutput "win-c.out" `
  -RedirectStandardError "win-c.err" `
  -PassThru

$p2 = Start-Process ./jmcl-core.exe `
  -ArgumentList @("instance","install","win-d","--instances-directory",$Instances,"--store-directory",$Store) `
  -RedirectStandardOutput "win-d.out" `
  -RedirectStandardError "win-d.err" `
  -PassThru

Wait-Process -Id $p1.Id,$p2.Id
```

Verify that:

- Both processes exit normally.
- No object lock remains permanently blocked.
- There are no hash errors or partial final objects.
- Both instances reference the same store objects.
- A shared object is not downloaded twice.

## 9. Interrupted Installation

Start an installation, terminate it while files are downloading, then run the
same command again.

Verify that:

- The second run completes successfully.
- Published store objects pass hash verification.
- Incomplete temporary files are not treated as valid objects.
- Instance destinations are either complete old files or complete new files.

## 10. Additional Filesystems And Paths

Repeat a smaller test where available:

- Keep Windows Defender enabled.
- Use a path containing spaces.
- Use a path containing Chinese characters.
- Put instances on exFAT and confirm copy fallback.
- Put the store on a network share and record whether JMCL copies or returns a
  clear filesystem error.

## 11. Failure Report

Include the following information when reporting a failure:

```text
Windows version and build
Filesystem type for every involved volume
Complete command line
stdout and stderr
jmcl-core.pdb
Instance logs/latest.log
fsutil hardlink list output
fsutil file queryfileid output
Windows Defender or Controlled Folder Access status
```

## 12. BMCLAPI Instance Override

Create an instance with the default official source, then override only its
installation source:

```powershell
./jmcl-core.exe instance create bmcl-test `
  --version 26.1 `
  --instances-directory $Instances

./jmcl-core.exe instance install bmcl-test `
  --instances-directory $Instances `
  --store-directory $Store `
  --source bmclapi
```

The output must identify `BMCLAPI` as the source. If mirror metadata is
unavailable, planning should transparently use Mojang metadata rather than fail.
Static files must still prefer BMCLAPI and retain verified Mojang fallback URLs.

The highest-priority tests are same-volume reuse, reinstallation, game launch,
cross-volume fallback, deletion isolation, and concurrent installation.

All tests must be run from an ordinary non-administrator PowerShell session.
Hard-link creation does not require elevation. If `AccessDenied` occurs, record
whether the directory is under `Program Files`, Controlled Folder Access, or
another ACL-protected location.

## 13. Diagnosing `reached unreachable code`

Separate Windows diagnostic builds are generated at:

```text
zig-out/windows-debug/bin/jmcl-core.exe
zig-out/windows-debug/bin/jmcl-core.pdb
zig-out/windows-release-safe/bin/jmcl-core.exe
zig-out/windows-release-safe/bin/jmcl-core.pdb
```

First run the Debug executable with one worker. Unlike earlier builds,
`--workers 1` now executes inline and does not create or join a worker thread:

```powershell
& ".\windows-debug\jmcl-core.exe" instance install win-a `
  --instances-directory $Instances `
  --store-directory $Store `
  --workers 1 `
  *> unreachable-debug.log
```

Then repeat with the ReleaseSafe executable and eight workers:

```powershell
& ".\windows-release-safe\jmcl-core.exe" instance install win-a `
  --instances-directory $Instances `
  --store-directory $Store `
  --workers 8 `
  *> unreachable-release-safe.log
```

Attach the complete panic stack, both logs, executable, and matching PDB. Also
state whether the failure happens before the first progress event, only during
fresh downloads, or when all files are already cached.
