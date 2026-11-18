import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';
import {Stage, Photo, Shade, Sticker, Footnote, headline, clamp, ink, cream, mint} from './visuals';

export const Surprise = () => {
  const f = useCurrentFrame();
  const rewind = f >= 65;
  return <Stage color={ink}>
    <Photo src={rewind ? 'dorm.png' : 'dorm-shock.png'} start={1.04} end={1.10} duration={65} />
    <Shade />
    <Sticker x={102} y={135} color={cream} opacity={interpolate(f, [6, 15, 55, 67], [0, 1, 1, 0], clamp)}>咦？你也在玩这个？</Sticker>
    <Sticker x={1270} y={695} color="#ffb2b1" rotate={3} opacity={interpolate(f, [22, 31, 55, 67], [0, 1, 1, 0], clamp)}>暂停！暂停！</Sticker>
    {rewind && <>
      <AbsoluteFill style={{backgroundColor: ink, opacity: interpolate(f, [65, 76, 96, 120], [0, .92, .92, .70], clamp)}} />
      <svg width="1920" height="1080" style={{position: 'absolute', opacity: interpolate(f, [65, 73, 115, 130], [0, .35, .35, 0], clamp)}}>
        {[0, 1, 2, 3, 4, 5, 6].map(i => <path key={i} d={`M${(i * 330 - f * 26) % 2200} 0 l-270 540 l270 540`}
          fill="none" stroke={mint} strokeWidth="65" />)}
      </svg>
      <div style={{position: 'absolute', top: 320, left: 100, right: 100, textAlign: 'center',
        opacity: interpolate(f, [77, 86], [0, 1], clamp)}}>
        <div style={{fontSize: 45, color: mint, fontWeight: 800, marginBottom: 28}}>倒带一下。</div>
        <div style={{...headline, fontSize: 106}}>这次，先安排好屏幕。</div>
      </div>
    </>}
    <Footnote />
  </Stage>;
};
