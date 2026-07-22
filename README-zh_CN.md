# JMCL Core

[English](README.md) | 简体中文

JMCL Core 是一个使用 Zig 编写的 Minecraft: Java Edition 启动器核心。它被设计为由独立的图形界面通过基于标准输入和标准输出的版本化 JSON Lines RPC 协议进行驱动。

项目目前提供协议和 CLI 基础、Mojang 元数据解析、完整的静态安装、基于内容寻址的硬链接存储、并发且经过校验的下载、安全的原生库解压、旧版资源映射、Java 运行时选择、启动规划，以及受监督的游戏进程执行。项目还提供可配置的 Microsoft 身份验证和不保存凭据的公开账户注册表；多版本 Java 的下载与管理计划在未来实现。

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
zig build run -- launch execute 26.1 --directory ./.minecraft --offline-name Player
zig build run -- instance create vanilla-26.1 --version 26.1 --name "Vanilla 26.1"
zig build run -- instance create fabric-1.21.1 --version 1.21.1 --fabric-loader 0.16.10 --name "Fabric 1.21.1"
zig build run -- instance list
zig build run -- instance install vanilla-26.1
zig build run -- instance install vanilla-26.1 --source bmclapi
zig build run -- instance launch vanilla-26.1 --offline-name Player
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
```

当省略 `--directory` 或 RPC 的 `directory` 字段时，安装、准备和启动命令默认使用 `./.minecraft`。静态运行时文件、原生库工作目录、日志、选项、存档和资源包均位于这一目录树中。

托管实例默认位于 `./instances` 下。每个实例在 `instance.json` 中固定一个 Minecraft 版本和下载源，并拥有独立的 `instances/ID/.minecraft` 目录树。使用 `--instances-directory PATH` 可选择其他实例集合。

实例默认使用记录在 `instance.json` 中的下载源。向 `instance install` 传递 `--source bmclapi` 可临时覆盖一次镜像源。使用 BMCLAPI 规划时，如果镜像元数据不可用，程序会自动通过 Mojang 重试；静态文件仍优先使用 BMCLAPI，并以官方源作为回退。

经过校验的不可变文件只会在 `./.jmclcore/objects` 下存储一次，并通过硬链接物化到 `.minecraft` 中。使用 `--store-directory PATH` 可选择其他存储位置。跨文件系统放置文件时会自动回退到原子复制。

默认使用 Mojang 官方服务。中国大陆用户可以传递 `--source bmclapi`，或在 RPC 参数中使用 `"source":"bmclapi"` 来使用 BMCLAPI。BMCLAPI 返回的内容仍会根据 Mojang 提供的 SHA-1 值进行校验。

当前范围和协议说明请参阅 `docs/mvp.md` 与 `docs/protocol.md`。
安装计划格式见 `docs/install-plan.md`。
下载执行流程见 `docs/installer.md`。
运行时准备流程见 `docs/preparer.md`。
Java 选择见 `docs/java.md`。
启动参数生成见 `docs/launch.md`。
实例管理见 `docs/instances.md`。
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
