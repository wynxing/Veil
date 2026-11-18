import {AbsoluteFill, Easing, Img, interpolate, staticFile, useCurrentFrame} from 'remotion';
import type {CSSProperties, ReactNode} from 'react';

export const ink = '#20273f';
export const cream = '#fff8e9';
export const mint = '#bce9db';
export const coral = '#ffb2b1';
export const font = '"Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", sans-serif';
export const clamp = {extrapolateLeft: 'clamp', extrapolateRight: 'clamp'} as const;
export const ease = Easing.bezier(0.22, 1, 0.36, 1);
export const headline: CSSProperties = {fontSize: 100, fontWeight: 900, lineHeight: 1.22,
  letterSpacing: -2, color: cream,
  textShadow: `5px 6px 0 ${ink}, -2px -2px 0 ${ink}, 2px -2px 0 ${ink}, -2px 2px 0 ${ink}`};

export const Stage = ({children, color = cream}: {children: ReactNode; color?: string}) =>
  <AbsoluteFill style={{backgroundColor: color, color: ink, fontFamily: font, overflow: 'hidden'}}>
    {children}
  </AbsoluteFill>;

export const Paper = () => <AbsoluteFill style={{opacity: .22,
  backgroundImage: 'radial-gradient(#aaa08d .8px, transparent .8px)', backgroundSize: '7px 7px'}} />;

export const Photo = ({src, start = 1, end = 1.035, duration = 150, origin = '50% 50%'}:
  {src: string; start?: number; end?: number; duration?: number; origin?: string}) => {
  const f = useCurrentFrame();
  return <AbsoluteFill style={{scale: interpolate(f, [0, duration], [start, end], clamp), transformOrigin: origin}}>
    <Img src={staticFile(src)} style={{width: '100%', height: '100%', objectFit: 'cover'}} />
  </AbsoluteFill>;
};

export const Shade = () => <AbsoluteFill style={{background: 'linear-gradient(180deg, rgba(18,23,44,.60), transparent 35%, transparent 68%, rgba(18,23,44,.68))'}} />;

export const Footnote = ({text = '原创动画 · 情境示意'}: {text?: string}) =>
  <div style={{position: 'absolute', right: 90, bottom: 54, color: cream, fontSize: 24, opacity: .85}}>{text}</div>;

export const Sticker = ({children, x, y, color = mint, rotate = -2, size = 40, opacity = 1}:
  {children: ReactNode; x: number; y: number; color?: string; rotate?: number; size?: number; opacity?: number}) =>
  <div style={{position: 'absolute', left: x, top: y, backgroundColor: color, color: ink,
    padding: '20px 30px', fontSize: size, fontWeight: 800, border: `4px solid ${ink}`,
    borderRadius: '26px 26px 26px 5px', boxShadow: `7px 8px 0 ${ink}`, rotate: `${rotate}deg`, opacity}}>{children}</div>;

export const Cursor = ({x, y, pulse = 0}: {x: number; y: number; pulse?: number}) =>
  <div style={{position: 'absolute', left: x, top: y, width: 75, height: 92, zIndex: 10}}>
    {pulse > 0 && <div style={{position: 'absolute', left: -46, top: -46, border: '5px solid #f0a580',
      width: 100, height: 100, borderRadius: '50%', scale: 1 + pulse * 1.6, opacity: 1 - pulse}} />}
    <svg viewBox="0 0 75 92" width="75" height="92"><path d="M8 7 L10 68 L29 53 L43 83 L57 76 L41 47 L67 44 Z"
      fill="#fff8e9" stroke={ink} strokeWidth="5" strokeLinejoin="round" /></svg>
  </div>;

// A native vector device illustration, not a screenshot of Veil or an image edit.
export const Monitor = ({active = 1, small = false}: {active?: number; small?: boolean}) =>
  <svg viewBox="0 0 800 510" style={{width: '100%', height: '100%', overflow: 'visible'}}>
    <ellipse cx="400" cy="486" rx="328" ry="15" fill={ink} opacity=".09" />
    <rect x="22" y="18" width="756" height="402" rx="24" fill="#353950" stroke={ink} strokeWidth="8" />
    <rect x="43" y="39" width="714" height="353" rx="10" fill="#25293b" />
    <g opacity={active}>
      <rect x="43" y="39" width="714" height="353" rx="10" fill={small ? mint : '#f9d4dc'} />
      {small ? <>
        <rect x="75" y="67" width="126" height="288" rx="8" fill="#6f9d95" />
        {[92, 133, 174, 215].map(y => <rect key={y} x="92" y={y} width="90" height="12" rx="6" fill={cream} />)}
        <rect x="235" y="78" width="310" height="26" rx="10" fill="#68998c" />
        {[140, 181, 222].map(y => <rect key={y} x="235" y={y} width="425" height="16" rx="8" fill="#8ab3a5" />)}
        <rect x="235" y="280" width="150" height="61" rx="14" fill={cream} />
        <rect x="414" y="280" width="150" height="61" rx="14" fill="#eeb7af" />
      </> : <>
        <circle cx="250" cy="178" r="104" fill="#ed94b5" />
        <path d="M190 165 C190 108 244 117 250 143 C274 109 322 123 313 165 C310 195 270 226 250 241 C227 223 190 195 190 165Z" fill={cream} />
        <rect x="419" y="93" width="250" height="21" rx="10" fill="#dd9aad" />
        <rect x="419" y="140" width="208" height="17" rx="8" fill="#dfb1bd" />
        <rect x="419" y="182" width="250" height="17" rx="8" fill="#dfb1bd" />
        <rect x="75" y="301" width="650" height="59" rx="13" fill={cream} opacity=".85" />
        <rect x="100" y="321" width="400" height="15" rx="7" fill="#e1b5bd" />
      </>}
    </g>
    <circle cx="400" cy="405" r="5" fill="#898798" />
    <path d="M358 423 H442 L451 468 H521 Q537 468 537 482 H263 Q263 468 279 468 H349 Z" fill="#353950" stroke={ink} strokeWidth="7" />
  </svg>;
