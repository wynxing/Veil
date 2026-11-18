import {Img, staticFile} from 'remotion';
import {Stage, Shade, headline, mint, cream, ink} from './scenes/visuals';

export const Cover = () => <Stage>
  <Img src={staticFile('dorm-rest.png')} style={{width: '100%', height: '100%', objectFit: 'cover'}} />
  <Shade />
  <div style={{position: 'absolute', top: 92, left: 100, ...headline}}>
    <div style={{fontSize: 88}}>你玩你的。</div>
    <div style={{fontSize: 104, color: mint, marginTop: 12}}>屏幕，由你决定。</div>
  </div>
  <div style={{position: 'absolute', bottom: 108, left: 106, width: 690, padding: '24px 30px',
    backgroundColor: cream, border: `5px solid ${ink}`, borderRadius: 26,
    boxShadow: `10px 11px 0 ${ink}`, display: 'flex', alignItems: 'center', gap: 26, rotate: '-2deg'}}>
    <Img src={staticFile('veil.svg')} style={{width: 139, height: 139}} />
    <div><div style={{fontSize: 95, fontWeight: 950, lineHeight: 1.05, letterSpacing: -3}}>Veil</div>
      <div style={{fontSize: 32, marginTop: 16, fontWeight: 700}}>Windows 屏幕保持关闭工具</div></div>
  </div>
  <div style={{position: 'absolute', right: 90, bottom: 70, color: cream, fontSize: 26}}>原创动画 · 情境示意</div>
</Stage>;
