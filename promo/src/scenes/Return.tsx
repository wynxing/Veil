import {interpolate, useCurrentFrame} from 'remotion';
import {Stage, Photo, Shade, Footnote, headline, mint, clamp} from './visuals';

export const Return = () => {
  const f = useCurrentFrame();
  return <Stage>
    <Photo src="dorm-rest.png" start={1} end={1.035} duration={155} origin="72% 52%" />
    <Shade />
    <div style={{position: 'absolute', left: 100, top: 93, ...headline,
      translate: `0 ${interpolate(f, [8, 25], [25, 0], clamp)}px`, opacity: interpolate(f, [8, 21], [0, 1], clamp)}}>
      <div style={{fontSize: 86}}>你玩你的。</div>
      <div style={{color: mint, fontSize: 97, marginTop: 12}}>屏幕，由你决定。</div>
    </div>
    <div style={{position: 'absolute', right: 169, top: 399, fontSize: 70, color: '#ffb2b1',
      textShadow: '3px 4px 0 #fff8e9',
      opacity: interpolate(f, [35, 52, 112, 145], [0, 1, 1, 0], clamp),
      translate: `0 ${interpolate(f, [35, 145], [0, -55], clamp)}px`}}>♥</div>
    <Footnote />
  </Stage>;
};
