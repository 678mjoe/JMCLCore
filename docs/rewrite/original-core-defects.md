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
| [ZIG-008](#zig-008) | Windows loader verbatim 路径混用分隔符导致准备失败 | 原生 neo/success 与最小路径实验已确认 | 75 个引擎 / 12 个 RPC 已观察差异；仅显式精确案例门禁 |

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


<a id="zig-008"></a>
## ZIG-008 — Windows loader 的 verbatim 路径混用分隔符

**分类：确认缺陷，覆盖范围仍在收集。**

- **原生观察：** [Windows job 120917757037](https://github.com/678mjoe/JMCLCore/actions/runs/40109394068/job/120917757037)，重写提交 `090bf3`：修正测试用原生 Java 替身的目录比较后，loader 测试 10/12 通过。引擎 `neo/success` 中，Rust 执行两个合成 processor 并产生 SHA1 `d2a04d71301a8915217dd5faf81d12cffd6cd958`、大小 6 的输出；固定 Zig oracle 在首个 processor event 前返回 `BadPathName`。RPC 对应 Rust 完成而 Zig 返回 `PROCESSOR_FAILED`。这不是实际 Minecraft/Java 安装验收。
- **最小隔离实验：** `windows_verbatim_join_diagnostic` 在临时目录中，使用原版 `hard_link.absolutePathAlloc` 获取 verbatim 绝对路径，再按原版方式 join `.jmcl-path-probe/nested`。保留混合分隔符的 `createDirPath` 返回 `BadPathName`；仅将 `/` 换为原生反斜杠的对照成功。测试输出两条完整路径与准确错误；实际 processor oracle 和固定基线没有改写。
- **源码链条：** 固定基线 `src/install/hard_link.zig:149–161` 从目录句柄取得 Windows 绝对路径；`src/loaders/neoforge_executor.zig:204–240` 把它与含 `/` 的 Maven 父目录或工作目录 join 后交给 `cwd().createDirPath`。Zig 0.16 的 join 不重写片段内部的分隔符，而 verbatim 路径处理保留这些字符。该准备阶段在 processor 循环之前。`src/loaders/forge_executor.zig` 的客户端/服务端入口也调用同一 `runLayout`，但是否触发仍依赖各案例的前置验证和缓存；不能将全部 Forge 输入称为已观察失败。
- **独立于 ZIG-003：** 此处没有进入最终成功响应，不能用释放后 hash 的窄例外去接受 `BadPathName`、缺失 processor event 或错误 receipt。Rust 不应为模仿缺陷而破坏路径或故意中断正常合成链。
- **完整原生矩阵：** [Windows job 120923637973](https://github.com/678mjoe/JMCLCore/actions/runs/40111376472/job/120923637973)，提交 `4a7f902`，采集 92 个引擎案例（Unix 非 UTF-8 四案例在 Windows 不适用）与 24 次 RPC。引擎 75 个差异全部为原版准确单条 `BadPathName`，17 个保持严格一致：每种模式的 `state-valid`、`state-uppercase`、`installer-missing`、`profile-missing`，以及 `forge-server/server-hash-missing`。RPC 差异仅为 `neo|forge × success|failed|hash-mismatch × run0|run1` 共 12 次，其余 12 次保持严格一致。CLI 没有新增例外。
- **精确门禁：** 仅 Windows x86_64 且 `JMCL_ALLOW_ZIG008_LOADER_PATHS=1` 时，允许 [`zig008.json`](../../rust/tests/fixtures/loaders/zig008.json) 中逐一列明的 75/12 行。每行包含原版准确记录与磁盘快照、Rust 准确记录与磁盘快照，以及 RPC 双方准确 receipt 字节；任一不同即失败，不接受通用 `BadPathName` 或 `PROCESSOR_FAILED`。若命名案例双方实际一致，仍走严格相等路径，不强制原版失败。未设置开关仍为严格差分。诊断开关 `JMCL_DIAGNOSE_WINDOWS_LOADER_MATRIX=1` 本身仍不接受差异；不能将此门禁称为完整兼容。
- **预期值独立验证：** 这些 Rust 预期不是只根据 Windows 当前实现生成后直接认可：全部命名案例还必须在 Linux 对固定 Zig 基线进行实际严格差分，并逐字段核对同一预期记录、state、输出、顺序和 receipt；RPC 原有 ZIG-003 只校正其既定 Debug 释放后 hash 模式。跨主机 receipt 只根据明确目标替换预期 `os`/`arch`，不归一化其它字段；Windows 接受门禁仍比较未修改的原生 receipt。Linux 验证不是 Windows 门禁的原生运行结果。
- **拒绝对照：** 每个命名行逐字段删除、增加字段、逐叶值变异（包括事件、错误、hash、state、顺序、receipt）必须被匹配器拒绝；未知模式/场景、重复行及上述 17 个严格案例不能进入例外。真实文件检查另有私有副本及可写硬链接负对照，避免 JSON 正确而 CAS 共享身份错误。
- **Rust 正向约束：** `synthetic_native_chain_and_state` 必须执行真实合成子进程，验证输出内容、SHA1、大小、state、工作目录清理和缓存；CAS 对象及硬链接检查不能被 oracle 的原版失败取代。异常路径继续要求准确错误、事件顺序及没有错误的完成 receipt。

<a id="zig-009"></a>
## ZIG-009 — 已打开的 Linux 进程身份文件在退出后读取返回 ESRCH

**分类：确定性合成复现；Rust 窄修复，固定 Zig 基线不改写。**

- **观察边界：** Linux CI [job 120941080105](https://github.com/678mjoe/JMCLCore/actions/runs/40117291067/job/120941080105) 的合成停止升级测试曾返回 `SERVER_STOP_FAILED` 而非 `terminated`。该日志未记录底层 errno，不能声称已经直接追踪到本次 CI 失败的系统调用。以下是独立复现、与该症状相符的一条确定失败路径，不是对全部生命周期竞态的解释。
- **确定性复现：** 只启动并持有自己的阻塞式 `/bin/sh` 子进程，确认 cmdline 中包含指定的 JVM 身份参数，打开 `/proc/<pid>/cmdline`，随后杀死并等待该子进程退出，再从保留的描述符读取。Linux 返回 `ESRCH`（errno 3），而不是重新按路径打开时的 `ENOENT`。回归测试 `identity_read_after_owned_child_exit_is_not_a_stop_failure` 在修复前准确失败于此 `ESRCH`，修复后返回不匹配；不依靠高频碰撞获得覆盖。
- **原版与迁移行为：** 固定 `5a4310d` 的 `src/server/lifecycle.zig:646–680` 仅将 open 的 `FileNotFound` 视为消失，read 的所有非 `INTR` 错误都会成为 `ReadProcessIdentityFailed`，经停止接口映射为 `ServerStopFailed`。Rust 原迁移同样传播该 read 错误。因此成功终止的进程也可能触发失败响应。
- **窄修复：** 仅已打开身份描述符的 read 返回 `ESRCH` 时报告 `false`（没有匹配进程）。它绝不证明身份、绝不允许发送信号；部分匹配字节后再遇到 `ESRCH` 也必须返回 false。`EACCES`、`EIO` 等错误仍保留错误，活进程的完整身份匹配、错误身份、空 cmdline 与 64 KiB 上限规则不变。没有放宽 PID 校验、改变信号顺序或延长停止期限。
- **独立的测试初始化竞态：** 启动握手只表示子进程已生成，不保证应用脚本已经安装 TERM trap。升级测试现在有界等待脚本在 trap 安装之后写出的 `synthetic-start`，再断言必须由 TERM 或 KILL 终止；这只固定测试前提，不修改生产启动握手或掩盖 read 错误。
- **验证：** 窄回归的两个测试先红后绿；生命周期单元测试 11/11、完整合成生命周期集成测试 8/8 通过。固定 TERM/KILL 升级用例额外连续运行 8 次全部通过，每次包含两个自有子进程；这些有限重复只补充稳定性证据，不能替代确定性回归或承诺永不再现任何竞态。`cargo +1.99.1 clippy --all-targets -- -D warnings` 通过。
- **范围：** 所有进程、目录和状态均为一次性本地合成夹具；没有真实 Minecraft/Java、SSH、用户服务或 CI 盲重试。此修复是已记录的缺陷修正差异，不能称为与原版所有竞态结果完全一致。

## 源码确认的独立风险：带凭据请求的跨源重定向

此项与上面已收集运行观察的条目分开记录；**没有执行网络实验，也没有证据表明真实密钥已经泄露**。

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
