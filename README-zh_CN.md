# JMCL Core

[English](README.md) | 简体中文

JMCL Core 是一个使用 Zig 编写的 Minecraft: Java Edition 启动器核心。它被设计为由独立的图形界面通过基于标准输入和标准输出的版本化 JSON Lines RPC 协议进行驱动。

项目目前提供协议和 CLI 基础、Mojang 元数据解析、完整的静态安装、基于内容寻址的硬链接存储、并发且经过校验的下载、安全的原生库解压、旧版资源映射、支持 Mojang 与 Azul Zulu 托管运行时下载的 Java 运行时选择、启动规划，以及受监督的游戏进程执行。项目还提供可配置的 Microsoft 身份验证和不保存凭据的公开账户注册表。

## 注意

这是一个 vibe coding 项目。

## 说明

JMCL Core 是一个人人可用的 Minecraft: Java Edition 启动器后端。若想定制专属于你的启动器，只需将 `docs/gui-handoff.md`、本仓库链接和编译好的 `jmcl-core` 交给你的 AI Agent。正如 Minecraft 中的进度所言：“Sky's the Limit”。

## 环境要求

- Zig 0.16.0

## 构建与测试

```sh
zig build
zig build test
```

当前小范围可靠性修复计划见 [ROADMAP.md](ROADMAP.md)。

## CLI

```sh
zig build run -- version
zig build run -- version --json
zig build run -- version list --type release --limit 10
zig build run -- version list --type snapshot --limit 5 --json
zig build run -- version list --source bmclapi --limit 10
zig build run -- version show 1.21.11
zig build run -- version show 1.6.4 --json
zig build run -- version show 1.21.11 --source bmclapi
zig build run -- install plan 1.21.11
zig build run -- install plan 1.6.4 --os windows --arch x86_64 --json
zig build run -- install execute 1.21.11 --directory ./.minecraft --workers 8
zig build run -- install prepare 1.6.4 --directory ./.minecraft
zig build run -- store gc --store-directory ./.jmclcore
zig build run -- java detect
zig build run -- java select 26.2
zig build run -- java runtime list
zig build run -- java runtime install --component java-runtime-gamma
zig build run -- java runtime install --major 8
zig build run -- java runtime remove mojang-java-runtime-gamma-mac-os-arm64
zig build run -- launch execute 26.1 --directory ./.minecraft --offline-name Player
zig build run -- instance create vanilla-26.1 --version 26.1 --name "Vanilla 26.1"
zig build run -- instance create fabric-1.21.1 --version 1.21.1 --fabric-loader 0.16.10 --name "Fabric 1.21.1"
zig build run -- instance list
zig build run -- instance install vanilla-26.1
zig build run -- instance install vanilla-26.1 --source bmclapi
zig build run -- instance launch vanilla-26.1 --offline-name Player
zig build run -- instance worlds list vanilla-26.1
zig build run -- instance worlds show vanilla-26.1 "New World"
zig build run -- instance worlds rename vanilla-26.1 "New World" "Survival Run"
zig build run -- instance worlds duplicate vanilla-26.1 "Survival Run" "Backup"
zig build run -- instance worlds delete vanilla-26.1 "Backup"
zig build run -- instance worlds export vanilla-26.1 "Survival Run" /tmp/survival.zip
zig build run -- instance worlds backup vanilla-26.1 "Survival Run" --label "pre boss"
zig build run -- instance worlds restore vanilla-26.1 "Survival Run-20240102-030405-pre-boss.zip" --replace
zig build run -- server worlds backup fabric-1.21.1 --label full
zig build run -- server worlds list fabric-1.21.1
zig build run -- server create fabric-1.21.1 --version 1.21.1 --fabric-loader 0.16.10 --name "Fabric Server"
zig build run -- server list
zig build run -- server show fabric-1.21.1 --json
zig build run -- server install fabric-1.21.1 --accept-eula
zig build run -- server properties get fabric-1.21.1
zig build run -- server properties set fabric-1.21.1 motd="JMCL Core Server"
zig build run -- server start fabric-1.21.1
zig build run -- server status fabric-1.21.1 --json
zig build run -- server command fabric-1.21.1 "say hello from jmcl"
zig build run -- server logs fabric-1.21.1 --max-bytes 65536
zig build run -- server restart fabric-1.21.1 --grace-ms 10000
zig build run -- server stop fabric-1.21.1
zig build run -- rpc
```

RPC 模式从标准输入逐行读取 JSON 请求，并将 JSON 事件逐行写入标准输出。RPC 模式运行期间，日志和诊断信息绝不能写入标准输出。

```json
{"protocol":1,"id":"request-1","method":"ping"}
{"protocol":1,"id":"request-2","method":"core.version"}
{"protocol":1,"id":"request-3","method":"version.list","params":{"type":"release","limit":10}}
{"protocol":1,"id":"request-4","method":"version.get","params":{"id":"1.21.11"}}
{"protocol":1,"id":"request-5","method":"version.get","params":{"id":"1.21.11","source":"bmclapi"}}
{"protocol":1,"id":"request-6","method":"install.plan","params":{"id":"1.21.11","source":"bmclapi","os":"windows","arch":"x86_64"}}
{"protocol":1,"id":"request-7","method":"install.execute","params":{"id":"1.21.11","directory":"./.minecraft","source":"bmclapi","workers":8}}
{"protocol":1,"id":"request-8","method":"install.prepare","params":{"id":"1.6.4","directory":"./.minecraft","os":"windows","arch":"x86_64"}}
{"protocol":1,"id":"request-9","method":"java.detect"}
{"protocol":1,"id":"request-10","method":"java.select","params":{"id":"26.2"}}
{"protocol":1,"id":"request-11","method":"launch.plan","params":{"id":"26.2","directory":"./.minecraft","natives_directory":".jmcl/runtime/natives/session-id","auth":{"player_name":"Player","uuid":"uuid","access_token":"secret"}}}
{"protocol":1,"id":"request-12","method":"launch.execute","params":{"id":"26.2","directory":"./.minecraft","auth":{"player_name":"Player","uuid":"uuid","access_token":"secret"}}}
{"protocol":1,"id":"request-13","method":"java.runtime.install","params":{"major":17}}
{"protocol":1,"id":"request-14","method":"java.runtime.list"}
```

当省略 `--directory` 或 RPC 的 `directory` 字段时，安装、准备和启动命令默认使用 `./.minecraft`。静态运行时文件、原生库工作目录、日志、选项、存档和资源包均位于这一目录树中。

托管实例默认位于 `./instances` 下。每个实例在 `instance.json` 中固定一个 Minecraft 版本和下载源，并拥有独立的 `instances/ID/.minecraft` 目录树。使用 `--instances-directory PATH` 可选择其他实例集合。

实例默认使用记录在 `instance.json` 中的下载源。向 `instance install` 传递 `--source bmclapi` 可临时覆盖一次镜像源。使用 BMCLAPI 规划时，如果镜像元数据不可用，程序会自动通过 Mojang 重试；静态文件仍优先使用 BMCLAPI，并以官方源作为回退。

经过校验的不可变文件只会在 `./.jmclcore/objects` 下存储一次，并通过硬链接物化到 `.minecraft` 中。使用 `--store-directory PATH` 可选择其他存储位置。跨文件系统放置文件时会自动回退到复制；此路径不是原子替换，后续安装会重新校验。详见[存储边界](docs/content-addressed-storage.md#atomic-materialization)。

托管 Java 运行时会从 Mojang 运行时目录下载到制品存储的 `runtimes/` 目录下；对于 Mojang 未提供的平台（例如 macOS arm64 上的 Java 8）会回退到 Azul Zulu。启动和安装命令接受 `--java-policy auto|local|managed`（RPC `java_policy`）来控制本地与托管运行时的优先级，并支持 `--java-override`（RPC `java_override`）强制指定 Java 可执行文件。

默认使用 Mojang 官方服务。中国大陆用户可以传递 `--source bmclapi`，或在 RPC 参数中使用 `"source":"bmclapi"` 来使用 BMCLAPI。BMCLAPI 返回的内容仍会根据 Mojang 提供的 SHA-1 值进行校验。

存档/世界管理支持带 `level.dat` 解析元数据(名称、游戏版本、游戏模式、体积、图标、会话锁)的列表、崩溃安全的重命名/复制/删除、zip 导出/导入,以及 `backups/` 下的时间戳备份与恢复。存档是可变的实例私有状态,绝不进入制品存储。服务端提供相同的归档流程,外加要求停机的主世界重命名/删除(联动维度目录)。详见 `docs/worlds.md`。

Linux 服务器管理现已支持独立服务器清单、经过校验的原版、Fabric、NeoForge
和现代 Forge 服务端安装、原子化 `server.properties` 管理、CLI 命令，以及分离式
启动、停止、重启、状态、日志和命令 RPC。旧版 Forge 服务端支持仍然推迟。运行中的服务器由 SSH/JSON Lines 会话之外的监督进程管理，并提供 PID
身份校验、控制台 FIFO、有限日志轮换以及从优雅停止到 SIGKILL 的升级机制。

当前范围和协议说明请参阅 `docs/mvp.md` 与 `docs/protocol.md`。
安装计划格式见 `docs/install-plan.md`。
下载执行流程见 `docs/installer.md`。
运行时准备流程见 `docs/preparer.md`。
Java 选择与托管运行时见 `docs/java.md`。
启动参数生成见 `docs/launch.md`。
实例管理见 `docs/instances.md`。
存档/世界管理见 `docs/worlds.md`。
Linux 优先的服务器管理功能见 `docs/server.md`。
共享制品存储见 `docs/artifact-store.md`。
基于内容寻址的硬链接存储架构见 `docs/content-addressed-storage.md`。
Fabric Loader 支持见 `docs/fabric.md`。
NeoForge 支持见 `docs/neoforge.md`。
Forge 支持见 `docs/forge.md`。
Modrinth 模组和资源包管理见 `docs/content.md`。
Microsoft 身份验证边界及建议的 RPC 见 `docs/authentication.md`。
公开账户注册表和凭据所有权见 `docs/accounts.md`。
真实已批准客户端的身份验证流程见 `docs/authentication-verification.md`。

## 许可证

JMCL Core 是自由软件，采用 GNU Lesser General Public License 第 3 版或（由你选择）任何后续版本（`LGPL-3.0-or-later`）授权。完整条款请参阅 `LICENSE` 和 `COPYING`。
