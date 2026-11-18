import {interpolate, useCurrentFrame} from 'remotion';
import {Stage, Photo, Shade, Sticker, Footnote, headline, clamp, mint} from './visuals';

export const Night = () => {
  const f = useCurrentFrame();
  return <Stage>
    <Photo src="night-desk.png" start={1} end={1.035} duration={170} origin="60% 48%" />
    <Shade />
    <div style={{position: 'absolute', left: 100, top: 77, ...headline,
      translate: `0 ${interpolate(f, [8, 23], [22, 0], clamp)}px`, opacity: interpolate(f, [8, 20], [0, 1], clamp)}}>
      <div style={{fontSize: 80}}>夜深了，</div>
      <div style={{color: mint, fontSize: 95}}>闲置副屏也该下班。</div>
    </div>
    <Sticker x={1110} y={834} size={41} opacity={interpolate(f, [40, 53], [0, 1], clamp)}>留下正在用的那块。</Sticker>
    <Footnote />
  </Stage>;
};
