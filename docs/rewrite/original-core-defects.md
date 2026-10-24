# 原版 Core 缺陷台账

这份台账用于 Rust 重写与以后 Zig 修复之间的对照，不代替[兼容性契约](compatibility.md)，也不是技术博客。硬链接共享不可变文件、实例可变文件隔离、既有 protocol v1 和磁盘布局仍是约束；修复实现缺陷不是重新设计接口的许可。

## 基线与记录规则

- **原版基线：** Core `0.1.0-dev`，提交 [`5a4310debbcee26b3d2391bab2339f79cd6ec52e`](https://github.com/678mjoe/JMCLCore/tree/5a4310debbcee26b3d2391bab2339f79cd6ec52e)，Zig `0.16.0`。以下“原版未修”均相对此固定基线，不推断将来 `main` 的状态。
- **记录日期：** 2026-10-24。Rust 状态来自重写期间已收集的源码和有限测试证据，不是全部平台、全部输入的认证。
- **已批准原则：** 保持正常行为兼容；对源码与测试支持的原版缺陷，允许 Rust 明确纠正并留下对照记录。归档边界另有[窄范围安全例外](archive-safety-decision.md)。批准不等于实现完成、回归通过或允许改写 Zig `main`。
- **分类：** “确认缺陷”须区分观察结果与原因推断；“兼容性特征”不是待修 bug；“未验证假设”不得升级为确认缺陷。修复完成时补入提交、准确测试名、平台和结果，不能把计划中的测试写成通过。
- **门禁：** 已批准差异仍不是逐字节一致。严格差分不得静默归一化；实验性例外必须限定具体案例和预期结果，并关联下列稳定 ID。`full_compatibility` 与 `replacement_allowed` 不因本台账变为 true。

## 当前确认缺陷索引

| ID | 问题 | 固定 Zig 基线 | Rust 状态与剩余门禁 |
|---|---|---|---|
| [ZIG-001](#zig-001) | 超长 JSONL 行清除下一条已缓冲请求 | 未修 | 已实现保留下一请求；三个明确差异仍须独立标注 |
| [ZIG-002](#zig-002) | 后代持有输出管道使启动结束通知延迟 | 未修 | 已实现直接子进程结束边界；后代晚到输出语义差异须保留 |
| [ZIG-003](#zig-003) | loader 最终响应读取已释放的 hash | 未修 | 持有 hash 至序列化；严格 RPC 差分仍不同 |
| [ZIG-004](#zig-004) | 动态 f64 转 u64 上界触发进程 abort | 未修 | 安全返回现有参数错误；不能称为退出行为一致 |
| [ZIG-005](#zig-005) | Windows 竞争中的 GC 锁返回错误而非等待 | 未修 | 保留互斥并等待；原版等待语义差异仍单列 |
| [ZIG-006](#zig-006) | Java 归档解包的 staging 写入边界不足 | 已有隔离证据 | 安全例外已批准；实现与验证尚未完成 |
| [ZIG-007](#zig-007) | Windows 禁用内容换版在只读硬链接重命名时失败 | 原生观察确认特定案例 | Rust 保持禁用状态并完成换版；差分须显式选择窄例外 |

<a id="zig-001"></a>
## ZIG-001 — 超长行后的有效请求被清除

**分类：确认缺陷。**

- **条件与症状：** 超长 JSONL 行之后的正常 `ping` 已经进入同一个输入缓冲区时，原版可能输出两次 `INVALID_REQUEST`，而不是拒绝超长行后正常回答下一条请求。影响同一 RPC 会话后续请求，不代表所有超长输入都导致此现象。
- **源码：** [`src/main.zig:75–102`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/main.zig#L75-L102)：`StreamTooLong` 分支丢弃当前行之后对整个 `stdin_buffer` 执行 `secureZero`，包括仍待处理的数据。
- **已有证据：** [`rust/compat/check.py`](../../rust/compat/check.py) 的 `line boundary 65536`、`line boundary 65537`、`line boundary 131072` 精确记录上述两个响应的差异；此前基础语料为 51 个完全相同案例及这三个明确差异。这里引用既有结果，没有新增运行。
- **预期与 Rust：** 拒绝过长请求，但保留其后已经缓冲的有效请求，同时继续清除已消费的敏感字节。Rust 已采取这一行为；实验性已知差异不是原版兼容性通过。
- **以后修 Zig：** 使清零范围与 reader 已消费范围一致；在同次输入中覆盖超长行、后续正常请求、多个后续请求及分块输入，并验证清零不破坏尚未消费字节。
- **不可混同：** 未终止超长行遇 EOF 时，原版退出 1、stderr 报 `EndOfStream`、没有 RPC event；Rust 已保留这一独立行为。本条不授权改变它。

<a id="zig-002"></a>
## ZIG-002 — 等待输出 EOF 先于确认直接子进程退出

**分类：确认缺陷；修正包含明确的输出生命周期选择。**

- **条件与症状：** 被启动程序退出后，其后代仍持有继承的 stdout/stderr 管道，Core 的结束结果会等待那些管道关闭。后代长期不关闭时，等待可以持续；这不是一般启动失败，也不证明 Core 自身死锁。
- **源码：** [`src/launch/process.zig:217–231`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/launch/process.zig#L217-L231) 先读到输出流 EOF，再执行 `child.wait`。
- **已有证据：** [`inherited_pipe_known_gap`](../../rust/tests/launch_process_differential.rs) 使用有界、无害子进程；[证据说明](../../rust/compat/launch-evidence.md)记录原版在直接子进程退出后仍等待的观察。该测试是差异证据，不是相同行为通过。
- **预期与 Rust：** 以直接子进程退出为结束边界，分批排空当时可读输出、完成 framing，然后关闭 Core 的管道句柄。Rust 已实现；**更晚的后代输出可能不再收集**。不引入猜测 PID、杀整个进程组或清理任意后代的策略。
- **以后修 Zig：** 分离直接子进程状态与管道 EOF 的等待；保留日志分帧、脱敏、退出码和已有可读输出，针对每个原生平台验证退出与 pipe 状态组合。不能只用缩短超时掩盖问题。

<a id="zig-003"></a>
## ZIG-003 — Forge / NeoForge 的 patched-client hash 生命周期过短

**分类：确认缺陷。**

- **条件与症状：** loader processor 成功生成 patched client 后，终态安装 RPC 响应可能不是有效 hash。隔离 fixture 中 Debug 分配器的释放填充值被序列化为 40 个 `170` 的数组，而非正确 SHA-1；不同构建不保证出现相同表现。
- **源码：** [`src/protocol/install.zig:310–343`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/protocol/install.zig#L310-L343)：loader 分支内 `defer free(patched.patched_sha1)`，但把借用 slice 赋给 `patched_client`，分支结束后才序列化最终响应。完成安装记录也发生在该最终序列化之前。
- **已有证据：** [loader 证据](../../rust/compat/loaders-slice.md)及 [`install_processor_protocol_matches_oracle`](../../rust/tests/loaders_differential.rs)。fixture 使用离线合成数据。实验性例外仅识别准确的释放填充形态，并另行验证 Rust hash 等于生成文件的 SHA-1；这不证明任意原版输出都可忽略。
- **预期与 Rust：** 对最终 JSON 序列化保持 hash 所有权。Rust 持有 owning summary 到输出结束，不仿造释放后读取。该有限观察不扩展为数据损坏或可利用性结论。
- **以后修 Zig：** 将所有权/释放位置延长到终态输出之后；对两个 loader 的正常成功响应、真实文件 hash、写出失败与 receipt 时序建立回归，不能仅检测 Debug poison。

<a id="zig-004"></a>
## ZIG-004 — 动态浮点 u64 上界检查后仍可 abort

**分类：确认缺陷。**

- **条件与症状：** `store.gc.min_age_seconds` 的某些未加引号浮点数先经 `std.json.Value` 转为 f64；数学上的 u64 最大值在 f64 中已舍入至 2^64。已记录两个边界输入均使原版退出 `-6`，stdout 为空，stderr 为整数转换越界 panic。语料使用空目录参数，在参数解析阶段结束，未执行 GC 文件操作。
- **源码：** Zig 0.16.0 `lib/std/json/dynamic.zig::Value.parseFromNumberSlice` 的有限浮点路径与 `lib/std/json/static.zig::innerParseFromValue:565–573`。将 `maxInt(u64)` 转为 f64 后也得到 2^64，上界比较放行，后续 checked `@intFromFloat` 越界。
- **已有证据：** [精确输入、源码推导及退出结果](../../rust/compat/protocol-edges.md)。对应 oracle SHA-256：`b9e628efc5b7ea34047ad25129a98756b6f8ec5d0be38193aee2620d87a82bfe`。已收集一次性观察；普通 141-case 协议语料和 342-case 数值 golden 的通过不能代替本项退出差异说明。
- **预期与 Rust：** 无法安全表示的动态输入走现有 `INVALID_PARAMS`，不终止服务。Rust `json_number::dynamic_raw` 的 u64 路径安全拒绝该上界；不故意重现 abort。
- **以后修 Zig：** 在执行转换之前采用不会因 f64 上界舍入而放行的检查，并覆盖边界两侧及错误优先级。不得统一把所有数字改为 f64：带引号的 `"18446744073709551615.0"` 走 typed binary128 路径，可表示有效 u64 最大值，仍须接受。

<a id="zig-005"></a>
## ZIG-005 — Windows 的竞争锁导致 GC 报错，而非等待

**分类：确认行为缺陷；底层原因仍标为源码支持的推断。**

- **条件与症状：** Windows 上 Rust 持有同一 digest 锁时运行原版 GC，观察到退出 1、summary `errors=1` / `deleted=0`、stderr 为空、对象仍保留。释放锁后新一轮原版 GC 退出 0、删除一个 8-byte 对象且 `errors=0`。
- **源码：** Core [`lockObject`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/install/artifact_store.zig#L71) 与 [`collectObject`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/install/artifact_store.zig#L260) 的锁错误计数。Zig 0.16.0 `lib/std/Io/Threaded.zig:9136` 起的 Windows `fileLock` 使用 `NtLockFile`，异步句柄下未处理 `STATUS_PENDING`；这是与观察一致的原因推断，**没有 syscall trace 证明具体返回路径**。
- **已有证据：** [`native_gc_and_digest_lock_match_zig`](../../rust/tests/store_differential.rs) 中 Windows 的准确输出、锁持有时对象保留、释放后重新 GC 的断言。
- **预期与 Rust：** 同一对象锁竞争期间维持互斥，阻塞路径应等待锁可用而非计为对象处理错误。Rust 保持锁等待；实验性门禁只识别上述具体原版结果。
- **影响边界：** 已证明的是等待语义差异，**不是锁绕过、并发删除或文件损坏**。对象保留及释放后成功是关键负面证据，不应删去。
- **以后修 Zig：** 以原生 Windows 测试确认异步锁完成处理；覆盖锁获得、竞争、释放、错误处理与对象不变性。需判断修复应在 Core 的句柄/锁适配还是固定工具链实现中完成，不能仅放宽测试断言。

<a id="zig-006"></a>
## ZIG-006 — Java runtime 归档的 staging 写入隔离不足

**分类：已有隔离观察支持的确认缺陷；Rust 修复与验证未完成。**

- **条件与影响：** 特制或被篡改的归档到达 runtime 解包路径时，仅检查条目名称不足以保证实际写入留在 staging 内：路径解析可能经过将目标重定向出去的符号链接。影响受运行进程权限约束；没有证据表明官方发行归档被篡改，也没有权限提升结论。
- **源码与已有证据：** 原版 `src/java/runtime_installer.zig` 约 1060 行使用的 Zulu 解包调用，以及固定 Zig 0.16.0 `lib/std/tar.zig:588–671`。此前仅在一次性隔离临时目录中观察到 staging 外写入。这一观察只覆盖上述隔离条件，不构成完整归档安全审计。
- **批准的预期：** 允许合法内部链接，禁止实际解包写入逃离 staging；详见[安全边界决策](archive-safety-decision.md)。这不是一刀切禁止 symlink，更不能据此拒绝安全的只读 manifest/world 别名。
- **Rust 状态：** 实现与验收仍待完成；不能因原则获批就把 `runtime.install`、下载后启动或归档防护标记为已验证。现有本地/已安装 Java 的执行证据不覆盖此路径。
- **以后修 Zig：** 审核解包期间每次写入和路径组件解析的目录边界，保留正常内部链接；通过安全的内部链接、拒绝案例和清理行为回归验证。应单独记录工具链依赖，不悄然改变正常归档/协议契约。

<a id="zig-007"></a>
## ZIG-007 — Windows 禁用内容换版未完成

**分类：原生观察确认的缺陷；具体系统调用原因仍是源码支持的推断。**

- **已观察条件：** 固定基线的注入元数据 service oracle 在 Windows `mods / modrinth / disabled` 合成案例中，先发布 `mods/new.jar` 的只读 CAS 硬链接并输出 file/progress，然后返回 `CONTENT_SET_VERSION_FAILED`。旧 `one.jar.disabled` 和 Ver1 registry 保留，新文件却未变成禁用文件。Rust 同一案例完成 `new.jar.disabled` 与 Ver2 registry，保持只读内容。这里注入的仅是元数据 fetch，不是改写 rename 或 registry；不能称为未经注入的线上请求。
- **证据与原因边界：** `content_install_wire::injected_version_switch_wire_and_disk` 的原生上下文诊断同时记录双方 wire、磁盘与只读属性。基线 `src/service.zig:3548–3565` 在下载之后、保存 registry 之前重命名禁用文件。固定 Zig 0.16.0 `lib/std/Io/Threaded.zig:7385–7415` 先以 `GENERIC_WRITE | DELETE` 打开只读源，之后才设置 rename 的 `IGNORE_READONLY_ATTRIBUTE`；该打开步骤失败是原因推断，尚无直接 NT 调用跟踪证明。Rust 的对应句柄不要求文件数据写权限。
- **批准的纠正：** 成功完成合法禁用内容换版，不因兼容此缺陷而增加数据写权限、清除共享 inode 的 readonly 属性、复制替代硬链接或保留意外启用的新文件。旧禁用文件移除、新 registry 更新和新禁用文件必须同时通过测试。
- **精确门禁：** 仅 Windows 且显式设置 `JMCL_ALLOW_ZIG007_READONLY_RENAME=1` 时，测试接受预先列明的四类内容 × 两个 provider 的 disabled 合成案例。它逐字节断言 Zig 的两条既定 file/progress 事件、固定错误和精确残留磁盘状态，同时逐字节断言 Rust 的成功响应与更新 registry；还验证新文件、CAS 对象和旁路共享链接的字节、真实文件身份及 readonly。任何别的错误、事件、文件或 registry 改变都失败。如果原版在某个组合成功，仍要求双方严格相等，不强制制造预期失败。不设置开关仍为严格差分；启用例外不表示完全兼容。
- **覆盖边界：** 最初原生观察仅覆盖上述 mods/Modrinth 案例；随后 [Windows 精确门禁运行](https://github.com/678mjoe/JMCLCore/actions/runs/40103030834/job/120898896518) 分别确认四类内容 × 两个 provider 的八个 disabled 案例均符合该错误/残留状态及 Rust 纠正不变量，其余 32 个案例仍严格相同。此处仅描述该缺陷测试，不能推断该工作流的其它测试或项目整体通过。Linux 仍运行真实 oracle 严格差分，并用明确合成的失败状态验证匹配器与负对照；这不是 Windows 验收替代品。错误码、额外事件、registry、额外文件、Rust 失败/错误字节、私有复制和可写文件均有拒绝对照。

## 源码确认的独立风险：带凭据请求的跨源重定向

此项与上面七条已收集运行观察的条目分开记录；**没有执行网络实验，也没有证据表明真实密钥已经泄露**。

- 固定基线 [`src/service.zig` 的 `fetchBodyWithHeader`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/service.zig#L4147) 允许三次重定向，并将 `x-api-key` 放入 `extra_headers`。固定 Zig 0.16.0 `lib/std/http/Client.zig:821–829` 的规则保留跨域 `extra_headers`，只移除 `privileged_headers`。
- [`src/install/https_fallback.zig:41–54`](https://github.com/678mjoe/JMCLCore/blob/5a4310debbcee26b3d2391bab2339f79cd6ec52e/src/install/https_fallback.zig#L41-L54) 的重定向循环也将同一 `extra_header` 传给后续 URL。这支持“凭据头可能被转发到不同源”的源码风险判断，但不等同于已观察到泄露。
- Rust [`content/http.rs`](../../rust/src/content/http.rs) 对带 key 的元数据请求保留 HTTPS `api.curseforge.com:443` 的源限制，不为了逐字节兼容而放开凭据转发。真实服务、TLS、跨源响应及错误映射的网络验收仍未进行。
- 未来修复应把敏感头的发送限制在明确授权的源，并独立核对主传输和 fallback 的重定向路径；正常无凭据 artifact 请求不能与带 key 的元数据请求混为一谈。

## 不列为缺陷的兼容性特征

- 已验证的 `instance.json` / `level.dat` **只读符号链接别名**可保持支持；读取并未修改外部目标。不能把“有 symlink”自动等同于 ZIG-006 的归档写入越界。
- typed binary128 与 dynamic binary64 数值路径的差异、数字字符串的合法语法及错误优先级本身是既有解析行为。只有明确的 abort 问题记为 ZIG-004；Rust 在其它调用处丢失精度是重写偏差，不是原版 bug。
- 硬链接引用计数 GC 与复制回退的不同生命周期是存储模型的一部分，不等于可达性 GC；本台账不授权改变硬链接优先的项目哲学。

## 已修复历史（不计入本基线的未修缺陷）

[PR #1](https://github.com/678mjoe/JMCLCore/pull/1) 已合并到上述 `5a4310d` 基线：

| 历史项 | 修复与证据边界 | 后续注意 |
|---|---|---|
| 覆盖导入/恢复世界缺少运行中锁检查 | 替换前加入 mutable/session-lock guard；已有合成负对照在修复前失败、修复后通过，检查原文件保留、过期锁允许和临时清理 | guard 的一次探测并不保证与随后外部启动游戏完全无竞态 |
| Windows 文件读取/句柄模式适配 | account manifest 与 preparer 采用相应原生读取；ZIP preflight/extraction 使用正确的异步句柄标记 | 属于已合入基线的修复，不能重复列为新发现 |

“所有写入原子化、随时终止完全安全”不是已证明保证：CAS 复制回退以及 Windows 删除后重建路径可能需要重验/修复，不能描述为自动事务回滚。未来发现新缺陷时新增稳定 ID；修复已有项时保留其历史、补入验证和修复引用，而非删除原始记录。
