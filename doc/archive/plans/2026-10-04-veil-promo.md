> [!NOTE]
> 本文为 2026-10-04 已完成的历史制作计划归档，仅作背景留存，不代表当前正式版口径。

# Veil 宣传片样片制作计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. 本次按用户已批准的分镜方向直接制作，在样片交付处保留创意评审节点。

**Goal:** 交付可播放的 8 秒开头样片及 45 秒全片分镜。

**Architecture:** 宣传素材独立放在 promo/，不改应用功能。原创宿舍插画作为主画面，Remotion 逐帧控制镜头、字幕和漫画图形；Node 生成简短原创合成器音轨。交付视频另存到主工作区内的忽略输出目录，避免清理工作树后丢失。

**Tech Stack:** Remotion 4.0.532、React 19、TypeScript、Node.js、FFmpeg。

**Spec:** promo/分镜.md

## Global Constraints

- 首轮只制作完整分镜和 8 秒样片，全片留待样片反馈。
- 全部开发和文档修改在新工作树及独立分支完成。
- 中文文档，原创成人角色，无现成游戏录屏，无实拍。
- 1080p、30 fps、240 帧；动画情境示意不作为功能和兼容性证据。

## Task 1: 工程、分镜与插画

**Files:** promo/package.json、promo/tsconfig.json、promo/分镜.md、promo/public/dorm.png、promo/素材说明.md。

- [x] 保存全片分镜及素材生成提示词。
- [x] 安装固定版本依赖：`npm.cmd install --no-audit --no-fund`。
- [x] 检查插画的角色、设备和初始近景裁切位置。

## Task 2: 8 秒动画与声音

**Files:** promo/src/index.ts、Root.tsx、Opening.tsx、promo/scripts/create-sound.mjs、promo/public/opening.wav。

- [x] 构建近景独享→镜头拉远→电脑分享→屏幕休息悬念的 240 帧镜头。
- [x] 执行 `npm.cmd run sound` 生成可重复的原创短音轨。
- [x] 执行 `npm.cmd run check`，修复类型错误。
- [x] 用 Remotion 生成近景、揭示和末帧静帧，检查中文与安全区。
- [x] 执行 `npm.cmd run render:sample` 和 `npm.cmd run still`。

## Task 3: 验证、保存与清理

**Files:** promo/README.md、promo/验证记录.md。

- [x] 用 ffprobe 确认尺寸、帧率、时长和音轨；用 FFmpeg 完整解码检查。
- [x] 保存验证记录。

### 已确定的收尾顺序

提交工程→确认主工作区仍干净→仅本次分支快进整合→复制视频和封面并核对 SHA256→保存可复用依赖→归档本次工作树→删除本次已整合分支。执行结果随最终交付报告；保留与本任务无关的工作树。
