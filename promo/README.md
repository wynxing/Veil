# Veil 宣传片工程

当前交付：完整分镜与 8 秒开头样片。完整宣传片尚未制作。

```powershell
cd promo
npm.cmd ci --no-audit --no-fund
npm.cmd run sound
npm.cmd run check
npm.cmd run studio
npm.cmd run render:sample
npm.cmd run still
```

视频输出 `out/veil-opening.mp4`，封面输出 `out/veil-opening-cover.png`。输出和依赖不进入 Git。

`src/Opening.tsx` 控制镜头、字幕和漫画图形；`public/dorm.png` 是原创插画；`scripts/create-sound.mjs` 生成原创音轨。文字使用 Windows 系统微软雅黑；其它系统需安装可用中文字体或修改字体列表。

情节为原创动画示意，不能作为实机兼容性或功能验收证据。完整脚本见 [分镜](分镜.md)，插画制作记录见 [素材说明](素材说明.md)。
