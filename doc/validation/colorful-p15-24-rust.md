# 验证结果：COLORFUL P15 24 上的 Rust 产品短时保持关闭

状态：**尚未机旁执行。** 不得把 Python 探针写成 Rust 已完成。

本页是 Rust 产品代码的验收摘要模板。系统检查与物理观察共同通过才算。API 成功单独不算。

| 项 | 记录 |
| --- | --- |
| 硬件 | COLORFUL P15 24，内屏 + S24Q6-Q24G8 |
| 软件 | `src/` Rust / egui；APPLY 仅 `Veil.Recovery`；本机禁止安装自带 MTT VDD |
| 已做 | 无 |
| 未做 | 短时只停内屏；`release` / 热键恢复；只停外屏；长时；循环；崩溃；睡眠 |

移植成功 ≠ 已验证。
