import {Img, interpolate, staticFile, useCurrentFrame} from 'remotion';
import {Stage, Paper, Cursor, Monitor, ink, cream, mint, clamp, ease} from './visuals';

export const Product = () => {
  const f = useCurrentFrame();
  const reveal = interpolate(f, [68, 90], [0, 1], {...clamp, easing: ease});
  const off = interpolate(f, [164, 176, 305, 315], [0, 1, 1, 0], clamp);
  const clickFrame = f < 250 ? 153 : 292;
  const cursorOpacity = interpolate(f, [116, 129, 324, 339], [0, 1, 1, 0], clamp);
  const cursorX = interpolate(f, [116, 153, 257, 292], [1705, 1510, 1510, 1450], {...clamp, easing: ease});
  const cursorY = interpolate(f, [116, 153, 257, 292], [850, 590, 590, 748], {...clamp, easing: ease});
  const pulse = f >= clickFrame && f < clickFrame + 20 ? (f - clickFrame) / 20 : 0;
  return <Stage>
    <Paper />
    <div style={{position: 'absolute', width: 780, height: 780, borderRadius: '50%', backgroundColor: mint,
      right: -200, top: -280, opacity: .5}} />
    <div style={{position: 'absolute', left: 130, top: interpolate(f, [66, 90], [265, 78], {...clamp, easing: ease}),
      scale: interpolate(f, [66, 90], [1, .55], {...clamp, easing: ease}), transformOrigin: '0 0', display: 'flex', alignItems: 'center', gap: 42,
      opacity: interpolate(f, [0, 15], [0, 1], clamp)}}>
      <Img src={staticFile('veil.svg')} style={{width: 235, height: 235}} />
      <div style={{fontSize: 204, fontWeight: 950, letterSpacing: -8}}>Veil</div>
    </div>
    <div style={{position: 'absolute', left: 140, top: 560, fontSize: 65, fontWeight: 900,
      opacity: interpolate(f, [10, 23, 65, 82], [0, 1, 1, 0], clamp)}}>给不需要的屏，放个假。</div>
    <div style={{position: 'absolute', left: 145, top: 672, fontSize: 42, color: '#64716d',
      opacity: interpolate(f, [21, 32, 65, 82], [0, 1, 1, 0], clamp)}}>Windows 屏幕保持关闭工具</div>

    <div style={{position: 'absolute', top: 254, left: 130, fontSize: 76, fontWeight: 900, opacity: reveal}}>
      {f < 147 ? '选中这块物理屏。' : f < 263 ? '保持关闭，直到主动恢复。' : '需要时，再恢复全部。'}
    </div>
    <div style={{position: 'absolute', left: 95, top: 435, width: 805, height: 515, opacity: reveal,
      translate: `${interpolate(f, [68, 90], [-70, 0], {...clamp, easing: ease})}px 0`}}>
      <Monitor active={1 - off} />
      <div style={{position: 'absolute', width: '100%', textAlign: 'center', top: 498, fontSize: 37, fontWeight: 800,
        color: off > .5 ? '#307c6c' : ink}}>{off > .5 ? '所选物理屏：保持关闭' : '所选物理屏：开启'}</div>
    </div>
    <div style={{position: 'absolute', left: 1045, top: 404, width: 725, height: 487, boxSizing: 'border-box',
      padding: '36px 42px', backgroundColor: '#fffdf5', border: `5px solid ${ink}`, borderRadius: 28,
      boxShadow: `11px 12px 0 ${ink}`, opacity: reveal,
      translate: `${interpolate(f, [68, 90], [70, 0], {...clamp, easing: ease})}px 0`}}>
      <div style={{fontWeight: 900, fontSize: 39}}>Veil <span style={{fontWeight: 500, fontSize: 29, color: '#758078'}}>／ 屏幕控制</span></div>
      <div style={{marginTop: 31, backgroundColor: '#edf4eb', borderRadius: 18, padding: '22px 24px',
        display: 'flex', justifyContent: 'space-between', alignItems: 'center'}}>
        <div><div style={{fontSize: 34, fontWeight: 800}}>桌面显示器</div><div style={{fontSize: 26, marginTop: 10, color: '#5f7168'}}>物理屏 1</div></div>
        <div style={{backgroundColor: off > .5 ? mint : cream, padding: '17px 21px', fontSize: 32,
          border: `3px solid ${ink}`, borderRadius: 14, fontWeight: 800}}>{off > .5 ? '已选择' : '保持关闭'}</div>
      </div>
      <div style={{marginTop: 24, fontSize: 27, color: '#5f7168'}}>屏幕状态，由你主动决定。</div>
      <div style={{marginTop: 30, textAlign: 'center', backgroundColor: mint, border: `3px solid ${ink}`,
        padding: '18px', borderRadius: 14, fontSize: 35, fontWeight: 800}}>恢复全部</div>
    </div>
    <div style={{opacity: cursorOpacity}}><Cursor x={cursorX} y={cursorY} pulse={pulse} /></div>
    <div style={{position: 'absolute', right: 90, bottom: 54, fontSize: 27, color: '#606960'}}>动画操作示意 · 非实机录像</div>
  </Stage>;
};
