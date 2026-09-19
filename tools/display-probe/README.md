# display-probe

Windows 11、64 位 Python 3.12 标准库技术探针，长期保留的实验室工具，不是产品应用。产品实现栈见 [技术架构](../../doc/ARCHITECTURE.md)。以下路径在仓库根执行。`out/` 不提交，正式实验建议把原始证据保存到主仓库 `.git/veil-validation-日期/`，避免清理工作树时丢失。

## 不关屏检查

```powershell
python tools/display-probe/probe.py enumerate
python tools/display-probe/validation.py diagnose --log out/diagnostic.jsonl
& tools/display-probe/collect-evidence.ps1 -OutputDirectory out/before
python -m unittest discover -s tools/display-probe -p test_validation.py -v
```

`diagnose` 核对 ABI，并对 basic、virtual、refresh 三组分别重新查询、校验原样拓扑及停用内屏拓扑，只使用 `SDC_VALIDATE`。零活动路径也允许诊断校验，结果不构成执行授权。日志保留标志、索引、原始结构字节和错误码。退出 0 只表示三组原样对照成功，不表示停用成功。

## 亮屏恢复与实际快捷键预检

```powershell
python tools/display-probe/validation.py preflight --config out/topology.json --receipt out/preflight.json --log out/preflight.jsonl
```

此命令先运行 2 秒定时恢复，再给出 45 秒快捷键窗口。看到 `preflight_wait` 的 `kind=hotkey` 后，在本机实际按 **Ctrl+Alt+Shift+F10**（功能键模式可能需要 Fn）。全程不主动关屏。F12 被 Windows 保留给调试器，不再作为候选。

两个恢复检查均成功，且保存前后拓扑相同，才生成 `ok=true` 的凭证。凭证有效期一小时，拓扑改变后失效。不得用注入按键伪装机旁预检。自动化注入只能证明消息处理链，在报告中单独标识。

旧的 `save`、`restore`、`watchdog --seconds N` 命令保留；单独运行这些命令不会签发关屏凭证。`watchdog` 现在同样等待独立进程注册快捷键并确认就绪。

## 关屏实验

仅在机旁有人、接电开盖、完成预检后执行；首次 15 秒。先保存工作，记录画面、背光、闪烁、布局变化。计时器在 S0 待机中不保证按墙钟执行，不能用软件看门狗替代机旁恢复。

```powershell
# 零活动路径也可校验；校验失败绝不 apply。
python tools/display-probe/probe.py disable-path --config out/topology.json --confirm off --target internal --validate-only --log out/validate.jsonl
python tools/display-probe/probe.py disable-path --config out/topology.json --confirm off --target external --validate-only --log out/validate-external.jsonl

# 需要第二活动目标以及有效凭证；虚拟屏安装后必须重新预检。
python tools/display-probe/probe.py disable-path --config out/topology.json --receipt out/preflight.json --confirm off --target internal --watchdog-seconds 15 --input-test --log out/off.jsonl

# 只用于电源行为诊断，不作为保持关闭方案。
python tools/display-probe/probe.py temp-off --config out/topology.json --receipt out/preflight.json --confirm off --watchdog-seconds 15 --log out/temp.jsonl

& tools/display-probe/collect-evidence.ps1 -OutputDirectory out/after
```

切换与恢复不使用 `SDC_SAVE_TO_DATABASE`。父进程在收到独立恢复进程就绪确认后才 arm，实际切换由该独立进程执行，避免父进程在恢复之后迟到执行关屏。恢复进程处理快捷键、定时器及异常；父进程崩溃不应终止恢复进程。恢复进程自身崩溃、驱动阻塞和系统待机仍非可靠恢复范围。

每次实验生成独立 `run-*` 目录：

- `ready.json` / `arm.json`：进程就绪与启动握手。
- `recovery.jsonl` / `result.json`：切换、每秒拓扑、恢复原因、返回码、目标与完整拓扑对比。
- `progress-ready.json` / `progress.jsonl`：另一个进程先确认完成首批工作，再允许关屏；随后每秒记录哈希工作量、摘要和墙钟间隔。间隔异常只能表明执行中断，需结合电源事件归因。

输入测试使用鼠标移动和 Shift，不输入文本。意外拓扑变化或循环出现超过 3 秒的间隔会触发提前恢复。`ok=true` 仅代表探针的系统检查成功；不代表物理屏幕验收通过。任何失败或父进程异常都会使预检凭证保持失效，须检查日志再重做预检。人工兜底为 `Win+Ctrl+Shift+B`，仍无画面再重启。

15 秒物理闭环通过后，才可把 `--watchdog-seconds` 改为 600 做 10 分钟测试。20 次循环逐次执行、逐次记录，不自动跳过物理观察或失败。短时恢复未通过时，不执行长时、睡眠唤醒和虚拟目标移除测试。

产品保持关闭见仓库 `src/`（Rust）。探针 worker `--seconds 0` 仍可用于实验室：直到热键、`release.json` 或父进程退出。

## 辅助脚本

安装脚本只用于本机已核验的签名包，必须管理员运行，拒绝覆盖已有 `C:\VirtualDisplayDriver` 或已有虚拟适配器。

```powershell
# 管理员：安装已核验的 VDD。证据目录须含 vdd\VirtualDisplayDriver 与 nefcon\x64\nefconc.exe
powershell -NoProfile -ExecutionPolicy Bypass -File tools/display-probe/install-vdd.ps1 -EvidenceDirectory <evidence>

# 汇总单次 run-* 目录的系统检查（不能代替机旁观察）
python tools/display-probe/summarize-run.py <run-directory> --output <summary.json>

# 至多 20 次 15 秒循环；任一次非 timer 成功即整组停止
python tools/display-probe/run-cycles.py --config <topology> --receipt <receipt> --directory <cycles-dir> --count 20 --seconds 15 --confirm off
```

