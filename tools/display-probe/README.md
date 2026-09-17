# display-probe

仅用于 Windows 显示控制技术验证的命令行探针，不是 Veil 产品应用。

依赖：Python 3.12 标准库。不依赖 pywin32。

```text
python probe.py enumerate
python probe.py save --config out/topology.json
python probe.py restore --config out/topology.json
python probe.py watchdog --config out/topology.json --seconds 20 --log out/watchdog.jsonl
python probe.py temp-off --config out/topology.json --watchdog-seconds 20 --confirm off --log out/temp-off.jsonl
python probe.py disable-path --config out/topology.json --watchdog-seconds 20 --confirm off --log out/disable-path.jsonl
```

关屏命令会先启动分离看门狗，到期后回放已保存拓扑。看门狗失败时使用 `Win+Ctrl+Shift+B` 或重启，这不是产品恢复能力。
