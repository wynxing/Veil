# 安装器 payload（不提交二进制）

将已核验的 Virtual Display Driver 25.7.23 与 NefCon v1.20.0 x64 放到本目录后再构建安装器。推荐运行 [`../FetchPayload.ps1`](../FetchPayload.ps1)，按 [`../payload.manifest.json`](../payload.manifest.json) 的 `sources` 与哈希下载并校验。缺失或哈希不符时，安装器构建必须失败，不得打出缺驱动的包。

## 布局

```text
installer/payload/
  vdd/mttvdd.cat
  vdd/MttVDD.dll
  vdd/MttVDD.inf
  nefcon/x64/nefconc.exe
```

哈希与发布者指纹见 [`../payload.manifest.json`](../payload.manifest.json)。

本目录二进制不进 Git。
