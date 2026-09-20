# 辅助虚拟输出：安装应用时一起装，没有就从面板装

> 实现时按任务顺序改。本页是产品合同变更后的代码计划，不是机旁已通过。

**目标：** 关最后一块物理屏时，用户能装上或启用辅助虚拟输出；不要再因为「不是自带的」把按钮禁用。

**原则：** 驱动文件始终随应用走。设备可以在安装应用时一起装，也可以稍后在面板里装。本机已有同一硬件 ID 的 MTT 就接管，不重装第二块。向日葵 / GameViewer 仍不作退路。

**技术：** 现有 Rust helper、WiX MSI、已校验 payload。不改上游驱动版本。

---

## 截图里为什么是死路

本机是 COLORFUL P15：内屏已保持关闭，外接 S2406-Q24G8 还亮着。再关外接就是关光全部物理屏，必须有第二活动目标。

面板走了 `KeepOffAction::Blocked`，文案是 `Gate::LAST_PATH_REASON`。这只在 `BundledVddAvailability::Absent` 时出现：注册表没有 `ROOT\MttVDD`，并且 `DriverStatus::payload_present()` 在 `%ProgramFiles%\Veil` 找不到 INF / nefcon。

当前机器上也没有 `installer/payload`。安装器默认其实会装设备，但：

- `INSTALLVDD=0` 会连文件都不拷（`BundledVdd` Feature 关掉）
- 开发目录跑的 `Veil.App` 只认 Program Files，不认仓库旁的 payload
- 已有未登记的 MTT 时 `install-driver` 直接失败，也不接管
- 没有 payload 时门禁用掉「保持关闭」，用户无法选择安装

旧合同把这写成「只用安装器自带；没有安装包就拒绝；已有 MTT 不接管」。那几句已经从产品文档删掉。

---

## 新合同（实现必须对齐）

1. 安装应用时，已校验的 MTT INF/DLL/CAT 与 NefCon **始终**写入 `%ProgramFiles%\Veil`。`INSTALLVDD=0` 只表示这次不创建设备，不表示不带文件。
2. 默认仍在安装时用 nefcon 装 `Root\MttVDD`，设备保持禁用。
3. 关最后一块物理屏时，「保持关闭」保持可点：已有设备则确认后启用；没有设备则确认后安装并启用。
4. 本机已有同一硬件 ID 的 MTT 实例：写入 `owned-devices.json` 接管，**不**再跑一遍 nefcon 创建设备，**不**再报「已有设备，停止安装」。
5. 只有「没有设备、也找不到已校验驱动包」才禁用最后一块的保持关闭，文案改成缺少驱动包，不要写「没有自带 VDD」。
6. 面板在没有设备时另给一条「安装辅助输出」，不必先点最后一块屏才发现能装。
7. 向日葵 / GameViewer 仍不进列表、不当第二目标。
8. 安装与启用仍要 UAC。不静默、不测签、不下载未核验包。
9. 卸载仍只移除 `owned-devices.json` 里的实例。接管来的设备也算 Veil 所有，卸时按清单删。

尚未机旁验证：P15 上装 MTT 后关光双物理屏、接管官方 VDD、开发目录旁 payload 补装。

---

## 要改的文件

- [src/veil-engine/src/coordinator.rs](../../src/veil-engine/src/coordinator.rs)：统一 payload 查找；`DriverStatus`
- [src/veil-engine/src/capability.rs](../../src/veil-engine/src/capability.rs)：门禁文案；必要时增加 `AdoptBundledVdd`
- [src/veil-engine/src/screen_list.rs](../../src/veil-engine/src/screen_list.rs)：最后一块可点；面板安装入口状态
- [src/veil-engine/src/tests.rs](../../src/veil-engine/src/tests.rs)：门禁、列表、协调器
- [src/veil-driver-helper/src/main.rs](../../src/veil-driver-helper/src/main.rs)：已有设备则接管
- [src/veil-app/src/main.rs](../../src/veil-app/src/main.rs)：确认框与「安装辅助输出」
- [installer/Veil.Setup/Package.wxs](../../installer/Veil.Setup/Package.wxs)、[Files.wxs](../../installer/Veil.Setup/Files.wxs)：文件始终随应用
- 产品文档已在本分支改；实现时同步用户可见字符串测试

---

### Task 1: 统一「找得到驱动包」

**文件：** `src/veil-engine/src/coordinator.rs`，必要时抽到 `capability.rs` 或新的 `payload.rs`

现在 `DriverStatus::payload_present()` 只看 Program Files。Helper 的 `resolve_payload()` 还会看 exe 旁和仓库 `installer/`。界面用前者，所以开发目录或文件没拷到 Program Files 时直接死路。

- [x] 把查找做成一处：按顺序试 `%ProgramFiles%\Veil`、exe 目录、exe 往上找到的 `installer/payload` 或 `installer`。文件齐全且能对上 `payload.manifest.json` 哈希才算有。
- [x] `DriverStatus::payload_present()` 与 helper `validate_payload` / `resolve_payload` 走同一套。
- [x] 单测：临时目录里放假 INF/nefcon/manifest 时为真；只有 Program Files 缺文件、但 exe 旁有文件时也为真（用注入路径，不要写真实 Program Files）。

```rust
pub fn payload_present_in(roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| payload_files_valid(root))
}
```

---

### Task 2: 已有 MTT 则接管，不重装

**文件：** `src/veil-driver-helper/src/main.rs`

删掉这段失败：

```rust
if !before.is_empty() {
    return Err("已有 MTT VDD 设备，无法证明由本次安装创建；保留原设备并停止安装。".into());
}
```

改成：

- [x] `before` 非空：`write_xml`（已有且内容相同则跳过；他人配置仍拒绝）、把现有实例写入 `owned-devices.json`、设备保持现状（不要为了「证明是我装的」再 disable/enable 一轮，除非当前是未知且用户正在启用）。
- [x] `before` 为空：现有流程，nefcon `install ... --no-duplicates`，记下新实例，装完禁用。
- [x] `enable`：`owned-devices.json` 有且实例仍在就启用。空清单但现场已有 `Root\MttVDD` 时，先接管再启用，避免「已装却不能用」。
- [x] `uninstall-driver`：仍只删清单里的实例。接管后的实例在清单里，卸载会去掉；这是新合同，须在文档和面板卸载说明里写清。
- [x] 单测装不到真 PnP 时，把「已有实例 → 写所有权、不调 nefcon」抽成纯函数测。

不要装第二块 `Root\MttVDD`。`--no-duplicates` 保留。

---

### Task 3: 最后一块屏保持可点

**文件：** `capability.rs`、`screen_list.rs`、`tests.rs`、`coordinator.rs`

- [x] `LAST_PATH_REASON` 改成：`没有第二活动目标，且找不到已校验的辅助虚拟输出驱动包，无法停用最后一条物理路径。`
- [x] `ENABLE_VDD_REASON` / `INSTALL_VDD_REASON` 去掉「安装器自带」，改成「将启用/安装辅助虚拟输出。这是显示驱动，拓扑可能短暂变化。」
- [x] `Absent` 才 `Blocked`。`PayloadOnly` 与 `Installed` 仍让 `can_keep_off == true`。
- [x] 协调器：`InstallBundledVdd` 先 `install-driver`（现已含接管）再 `enable`。成功前不改物理屏。
- [x] 改 `last_physical_without_bundled_vdd_is_blocked` 的文案断言。用户可见字符串测试一并改。

---

### Task 4: 面板上能直接安装

**文件：** `src/veil-app/src/main.rs`、`screen_list.rs`

关最后一块时的确认框还要保留。另外：

- [x] 没有已接管设备时，面板底部显示「安装辅助输出」。有 payload 则可点；点了走与 `InstallBundledVdd` 相同的确认 + helper，但不关屏。
- [x] 没有 payload 时按钮禁用，说明缺少已校验驱动包（安装器或 `installer/payload`）。
- [x] 已有设备则隐藏该按钮，或改成已安装、不可再点。
- [x] 确认框标题仍是 Veil。不要写「自带」。

---

### Task 5: 安装器始终带上驱动文件

**文件：** `installer/Veil.Setup/Package.wxs`、`Files.wxs`

- [x] `VddFiles` 挂到 `App` Feature，或让 `BundledVdd` 在 `INSTALLVDD=0` 时仍拷文件。
- [x] `INSTALLVDD=0` 只跳过 `InstallVdd` 自定义动作，不跳过文件。
- [x] `InstallVdd` 仍是 `NOT Installed AND INSTALLVDD=1`。失败仍不回滚应用文件（文件留下，面板可再装）。
- [x] 引导程序选项文案：不同意只表示现在不创建设备，以后仍可在面板安装。
- [x] 现有检查 WiX XML 的测试（`tests.rs` 里读 `Package.wxs`）按新条件改。

---

### Task 6: 验证与收口

- [x] `cargo test --manifest-path src/Cargo.toml`
- [ ] `installer/pack.ps1` 能编（本机有 payload 时）
- [ ] 机旁（未做不算完成）：
  1. 无 MTT 的机器：装应用（默认）后应有禁用态设备；关最后一块物理屏可确认启用。
  2. `INSTALLVDD=0`：Program Files 仍有 vdd/nefcon；面板「安装辅助输出」能装设备。
  3. 已有官方 MTT、尚无 `owned-devices.json`：点安装或关最后一块应接管，不出现第二块设备。
  4. 开发目录：`installer/payload` 齐时，未装 MSI 也能从面板安装。
  5. P15 关光双物理屏：现在允许装辅助输出；通过前不得写成已验证。

---

## 明确不做

- 不把向日葵 / GameViewer 当第二目标。
- 不在运行时从网上拉未核验 zip。
- 不改实验室 `tools/display-probe/install-vdd.ps1` 为产品安装器。
- 不把历史 Python 报告改成「当时已经这样装」。那些页只记录当时做了什么。
