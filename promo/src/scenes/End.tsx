import {Img, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {Stage, Paper, ink, mint, clamp, ease} from './visuals';

export const End = () => {
  const f = useCurrentFrame();
  return <Stage>
    <Paper />
    <div style={{position: 'absolute', top: -390, right: -130, width: 1250, height: 1250,
      borderRadius: '50%', backgroundColor: mint, opacity: .50}} />
    <div style={{position: 'absolute', left: 0, right: 0, top: 202, display: 'flex', gap: 34,
      justifyContent: 'center', alignItems: 'center',
      opacity: interpolate(f, [6, 18], [0, 1], clamp), scale: interpolate(f, [6, 25], [.91, 1], {...clamp, easing: ease})}}>
      <Img src={staticFile('veil.svg')} style={{width: 235, height: 235}} />
      <div style={{fontSize: 210, fontWeight: 950, letterSpacing: -9}}>Veil</div>
    </div>
    <div style={{position: 'absolute', top: 509, left: 90, right: 90, textAlign: 'center', fontSize: 56,
      fontWeight: 800, opacity: interpolate(f, [13, 27], [0, 1], clamp)}}>Windows 屏幕保持关闭工具</div>
    <div style={{position: 'absolute', top: 656, left: 345, right: 345, textAlign: 'center',
      backgroundColor: ink, color: '#fff8e9', borderRadius: 26, padding: '28px 30px',
      boxShadow: '9px 10px 0 #8bbeb0', opacity: interpolate(f, [21, 35], [0, 1], clamp)}}>
      <div style={{fontSize: 41, fontWeight: 800, marginBottom: 12}}>去 GitHub，看看 Veil →</div>
      <div style={{fontSize: 41, fontWeight: 600, letterSpacing: 1}}>github.com/wynxing/Veil</div>
    </div>
    <div style={{position: 'absolute', bottom: 99, left: 100, right: 100, textAlign: 'center',
      fontSize: 29, color: '#5c6863', opacity: interpolate(f, [23, 34], [0, 1], clamp)}}>
      Windows 11 x64 · 预览版 · 动画情境示意
    </div>
  </Stage>;
};
