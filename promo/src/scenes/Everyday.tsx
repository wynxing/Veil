import {interpolate, useCurrentFrame} from 'remotion';
import {Stage, Paper, Monitor, Sticker, ink, mint, coral, clamp} from './visuals';

export const Everyday = () => {
  const f = useCurrentFrame();
  const active = interpolate(f, [43, 60], [1, 0], clamp);
  return <Stage>
    <Paper />
    <div style={{position: 'absolute', top: -110, left: -150, width: 850, height: 850,
      borderRadius: '50%', backgroundColor: mint, opacity: .4}} />
    <div style={{position: 'absolute', top: 86, left: 125, fontSize: 98, lineHeight: 1.22,
      fontWeight: 900, opacity: interpolate(f, [8, 22], [0, 1], clamp)}}>
      <div>两块屏，</div><div>不必一起加班。</div>
    </div>
    <div style={{position: 'absolute', top: 462, left: 100, width: 810, height: 485}}><Monitor active={1} small /></div>
    <div style={{position: 'absolute', top: 462, left: 1010, width: 810, height: 485}}><Monitor active={active} small /></div>
    <Sticker x={255} y={378} size={36} rotate={-3} opacity={interpolate(f, [23, 35], [0, 1], clamp)}>这块，继续用。</Sticker>
    <Sticker x={1160} y={378} size={36} rotate={3} color={coral} opacity={interpolate(f, [56, 68], [0, 1], clamp)}>那块，先休息。</Sticker>
    <div style={{position: 'absolute', bottom: 64, left: 125, fontSize: 35, color: ink, fontWeight: 700,
      opacity: interpolate(f, [80, 95], [0, 1], clamp)}}>按物理屏选择，开或关。</div>
    <div style={{position: 'absolute', bottom: 58, right: 95, fontSize: 27, color: '#606960'}}>动画操作示意</div>
  </Stage>;
};
