# Veil 宣传片工程

当前交付：45 秒完整宣传片、封面、独立 SRT 字幕、完整分镜，以及保留的 8 秒开头样片。

```powershell
cd promo
npm.cmd ci --no-audit --no-fund
npm.cmd run sound
npm.cmd run sound:full
npm.cmd run captions
npm.cmd run check
npm.cmd run studio
npm.cmd run render:full
npm.cmd run cover:full
npm.cmd run render:sample
npm.cmd run still
```

完整视频输出 `out/veil-promo.mp4`，封面输出 `out/veil-promo-cover.png`，独立字幕输出 `out/veil-promo.srt`。样片保留为 `out/veil-opening.mp4`。输出和依赖不进入 Git。

`src/Promo.tsx` 组织 7 段场景；`src/scenes/` 分别控制情节、操作示意、日常场景及结尾；`src/Opening.tsx` 保留原开头。镜头及动画由帧驱动，字幕已写入画面；SRT 用于额外的字幕轨或后续发布。

`public/` 保存原创插画与项目既有标识。`scripts/create-sound.mjs` 生成 8 秒音轨，`scripts/create-full-sound.mjs` 生成 45 秒原创立体声配乐；没有配音。`scripts/render-checks.mjs` 生成关键检查静帧。文字使用 Windows 系统微软雅黑；其它系统需安装可用中文字体或修改字体列表。

情节为原创动画示意，不能作为实机兼容性或功能验收证据。完整脚本见 [分镜](分镜.md)，插画制作记录见 [素材说明](素材说明.md)。
