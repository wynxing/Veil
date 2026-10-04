import {Audio} from '@remotion/media';
import {AbsoluteFill, Easing, Img, interpolate, staticFile, useCurrentFrame} from 'remotion';
import type {CSSProperties} from 'react';

const clamp = {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'} as const;
const ink = '#20273f';
const cream = '#fff8e9';
const font = '"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", sans-serif';
const headline: CSSProperties = {
  fontWeight: 900, fontSize: 100, lineHeight: 1.24, letterSpacing: -2,
  color: cream, textShadow: '5px 6px 0 #20273f, -2px -2px 0 #20273f, 2px -2px 0 #20273f, -2px 2px 0 #20273f',
};

function Heart({x, y, delay, size}: {x: number; y: number; delay: number; size: number}) {
  const frame = useCurrentFrame();
  const t = frame - delay;
  return <div style={{position: 'absolute', left: x, top: y, color: '#ff89b7',
    fontSize: size, opacity: interpolate(t, [0, 10, 36, 54], [0, 1, 1, 0], clamp),
    translate: `0 ${interpolate(t, [0, 54], [0, -130], clamp)}px`,
    rotate: `${Math.sin(t / 10) * 12}deg`, textShadow: '3px 3px 0 #fff8e9'}}>♥</div>;
}

export const Opening = () => {
  const frame = useCurrentFrame();
  const intro = interpolate(frame, [0, 10, 77, 94], [0, 1, 1, 0], clamp);
  const reveal = interpolate(frame, [110, 120, 181, 191], [0, 1, 1, 0], clamp);
  const camera = interpolate(frame, [0, 74, 120, 240], [2.6, 2.52, 1, 1.025], {
    ...clamp, easing: Easing.bezier(0.22, 1, 0.36, 1),
  });
  const shock = frame >= 119 && frame < 128 ? Math.sin(frame * 2.8) * (128 - frame) : 0;

  return <AbsoluteFill style={{backgroundColor: ink, fontFamily: font, overflow: 'hidden'}}>
    <Audio src={staticFile('opening.wav')} />
    <AbsoluteFill style={{scale: camera, transformOrigin: '90% 47%', translate: `${shock}px 0`}}>
      <Img src={staticFile('dorm.png')} style={{width: '100%', height: '100%', objectFit: 'cover'}} />
    </AbsoluteFill>
    <AbsoluteFill style={{background: 'linear-gradient(180deg, rgba(18,23,44,.44), transparent 40%, transparent 70%, rgba(18,23,44,.62))'}} />

    <div style={{position: 'absolute', left: 100, top: 56, borderRadius: 30, padding: '10px 25px',
      backgroundColor: '#bce9db', border: `3px solid ${ink}`, color: ink, fontSize: 27,
      fontWeight: 700, rotate: '-2deg', opacity: intro}}>宿舍 · 23:47</div>
    <div style={{position: 'absolute', left: 100, top: 150, ...headline, opacity: intro,
      translate: `0 ${interpolate(frame, [0, 16], [28, 0], clamp)}px`}}>
      <div>你以为，</div>
      <div style={{color: '#bce9db', opacity: interpolate(frame, [16, 28], [0, 1], clamp)}}>只有你在看。</div>
    </div>

    <div style={{position: 'absolute', left: 106, top: 675, width: 775,
      padding: '28px 35px 30px', backgroundColor: cream, color: ink,
      border: `5px solid ${ink}`, borderRadius: '30px 30px 30px 5px',
      boxShadow: `9px 10px 0 ${ink}`, rotate: '-2deg',
      opacity: interpolate(frame, [24, 34, 77, 90], [0, 1, 1, 0], clamp),
      scale: interpolate(frame, [24, 35], [0.92, 1], clamp)}}>
      <div style={{fontSize: 26, color: '#af5177', fontWeight: 700, marginBottom: 12}}>好感度 +1 ♥</div>
      <div style={{fontSize: 52, fontWeight: 800}}>今天，也只看着我哦。</div>
    </div>
    {frame < 90 && <><Heart x={1460} y={390} delay={14} size={65} />
      <Heart x={1650} y={475} delay={33} size={44} /><Heart x={1530} y={600} delay={48} size={58} /></>}

    <div style={{position: 'absolute', left: 100, top: 74, ...headline, opacity: reveal,
      translate: `0 ${interpolate(frame, [110, 122], [20, 0], clamp)}px`}}>
      <div style={{fontSize: 86}}>但你的电脑，</div>
      <div style={{color: '#ffb2b1', fontSize: 103}}>正在替你分享。</div>
    </div>

    {frame >= 126 && frame < 188 && <svg width="1920" height="1080" style={{position: 'absolute',
      opacity: interpolate(frame, [126, 135, 180, 188], [0, 1, 1, 0], clamp)}}>
      <path d="M399 305 L949 271 L950 641 L399 654 Z" fill="none" stroke="#fff2bd"
        strokeWidth="9" strokeDasharray="22 14" strokeLinejoin="round" />
      <path d="M965 460 L1055 420 L1100 436" fill="none" stroke="#fff2bd" strokeWidth="7" strokeLinecap="round" />
    </svg>}
    <div style={{position: 'absolute', left: 1050, top: 358, color: ink, fontSize: 36,
      fontWeight: 900, padding: '17px 25px', borderRadius: '22px 22px 22px 3px',
      border: `4px solid ${ink}`, backgroundColor: '#fff2bd', boxShadow: `6px 7px 0 ${ink}`,
      rotate: '4deg', opacity: interpolate(frame, [134, 142, 178, 188], [0, 1, 1, 0], clamp)}}>
      全宿舍可见？！
    </div>
    <div style={{position: 'absolute', right: 300, top: 218, color: '#ffb2b1', fontSize: 123,
      fontWeight: 1000, rotate: '12deg', textShadow: `5px 6px 0 ${ink}`,
      opacity: interpolate(frame, [123, 130, 179, 188], [0, 1, 1, 0], clamp),
      scale: interpolate(frame, [123, 130, 136], [0.4, 1.15, 1], clamp)}}>!!</div>

    <div style={{position: 'absolute', inset: 0, backgroundColor: ink,
      opacity: interpolate(frame, [184, 204], [0, 0.66], clamp)}} />
    <div style={{position: 'absolute', left: 100, top: 380, right: 100, textAlign: 'center',
      opacity: interpolate(frame, [191, 201], [0, 1], clamp),
      translate: `0 ${interpolate(frame, [191, 206], [30, 0], clamp)}px`}}>
      <div style={{fontSize: 38, color: '#bce9db', fontWeight: 800, marginBottom: 25}}>等一下……</div>
      <div style={{...headline, fontSize: 94}}>这块屏，能不能先休息？</div>
      <div style={{marginTop: 34, color: '#fff8e9', fontSize: 28, letterSpacing: 6}}>VEIL</div>
    </div>
    <div style={{position: 'absolute', right: 90, bottom: 54, color: '#fff8e9',
      opacity: 0.8, fontSize: 22, letterSpacing: 2}}>原创动画 · 开头样片</div>
  </AbsoluteFill>;
};
