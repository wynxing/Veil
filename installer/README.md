# Veil 安装器

传统 WiX 5 Burn 引导 EXE + 应用 MSI。安装包没有 Authenticode。无签名预览的发布边界见 [doc/RELEASE.md](../doc/RELEASE.md)。

## 构建

1. 运行 `installer/FetchPayload.ps1`，或按 [payload/README.md](payload/README.md) 放入已核验的 VDD 与 NefCon。
2. 运行 `installer/ValidatePayload.ps1`：缺失或哈希/签名不符会失败。
3. 运行 `installer/pack.ps1`：用 `cargo build --release` 产出三个 exe（拷成 `Veil.*.exe`）并编译 MSI/Bundle。缺 payload 时会先 fetch。产物在 `installer/dist/`。
4. 打 `v*` 标签后由 CI 挂 GitHub prerelease；本机也可用 `installer/release.ps1`。Release 正文模板是 `installer/release-notes.template.md`。

无 payload 时不得打出缺驱动的包。版本、tag 与 GitHub prerelease 见 [doc/RELEASE.md](../doc/RELEASE.md)。

## 安装行为

- 提权后写入 `%ProgramFiles%\Veil`。
- 驱动同意：MTT 是显示驱动，用于关光全部物理屏时留下活动路径。默认一起装设备（禁用）。`INSTALLVDD=0` 只表示这次不创建设备，**驱动文件仍写入**，以后可在面板安装。
- INF 安装后设备保持禁用。
- 安装时不开机自启、不关屏。
- 卸载：先建立维护门禁、请求并确认物理屏恢复，再按安装记录的实例 ID 禁用和移除设备。恢复失败、未知、协议错误或超时停止卸载。提交/回滚动作释放门禁。

## vdd_settings.xml 路径

产品把 INF/DLL 放到 `%ProgramFiles%\Veil\vdd`，并把 `vdd_settings.xml` 同时写到该目录与 **`C:\VirtualDisplayDriver`**。对捆绑 `MttVDD.dll` 的只读字符串检查显示驱动写死后者。哈希与发布者指纹见 [payload.manifest.json](payload.manifest.json)。再分发许可见仓库根目录 `NOTICE`。

## 可靠性门禁（协议 v2）

- `Veil.App --restore-and-exit`：退出码 0 为恢复完成或无需恢复，1 为恢复失败/未知，2 为协议错误/超时。已结束的历史 `result.json`（含协议 v1）和没有恢复进程的目录不再当失败。MSI 不忽略仍在进行的恢复失败，也不忽略驱动移除失败。
- 升级：Burn 先跑 `retire-old`，复制已缓存的旧 MSI、把 `RestoreDisplays` 条件关掉再卸载，避免 0.1.5 及更早的恢复动作拦死 MajorUpgrade。新包自己卸载时，`UPGRADINGPRODUCTCODE` 不再跑 `RestoreDisplays`。`sweep-sessions` 仍会清掉已结束/弃守的 `%LOCALAPPDATA%\Veil\session-*`。
- 辅助 VDD 的 `install-driver` 失败不再回滚应用文件；文件留下，面板可以再装。没有第二活动目标时仍需要成功装上或接管设备才能关光最后一块物理屏。
- 维护标记存于 64 位 HKLM `Software\Veil\Maintenance`，由提权安装动作写入；全局命名互斥串行化门禁与关屏 APPLY。提交/回滚使用嵌入 MSI 的助手，删除安装目录后仍可清理标记。异常断电可能留下标记，此时拒绝关屏；应先通过安装器修复/完成维护，不能默默清除标记。
- 恢复检查只支持可确认的当前交互用户。SYSTEM 安装动作先确认只有一个登录用户，再通过该会话主令牌和用户环境启动固定的 App 恢复入口。其它用户仍登录（含断开的会话）、无登录用户或令牌/会话查询失败均阻止卸载；先在各用户会话恢复显示并注销其它用户。
- 设备实例写入 `%ProgramFiles%\Veil\owned-devices.json`。启用、禁用、移除只匹配此清单。本机已有同一 `Root\MttVDD` 时应写入清单并接管，不停止安装，也不新建第二块。向日葵 / GameViewer 仍不接管。
- 安装后禁用失败返回安装失败；清理限于本次新建实例和 XML。缺失/损坏的历史会话证明不能用一个 `ok=true` 绕过恢复门禁。

构建成功不等于安装、升级、卸载已在每台机器上实测。
