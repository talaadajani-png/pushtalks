import { useEffect, useRef, useState } from 'react';
import { on } from '../api';

interface State {
  state: 'recording' | 'processing' | 'done' | 'empty' | 'error' | 'idle';
  message: string | null;
}

const BARS = 9;
// Middle bars move most, like a voice waveform.
const SHAPE = [0.35, 0.55, 0.75, 0.92, 1, 0.92, 0.75, 0.55, 0.35];

// The floating dictation capsule. It never takes focus (see indicator.rs). While recording it
// shows only sound waves driven by the live mic level.
export default function Indicator() {
  const [s, setS] = useState<State>({ state: 'idle', message: null });
  const level = useRef(0);
  const bars = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    const offState = on<State>('dictation-state', (next) => {
      level.current = 0;
      setS(next);
    });
    const offLevel = on<number>('dictation-level', (l) => {
      // Speech RMS is small; lift it into 0..1 and react fast to rises.
      const v = Math.min(1, Math.sqrt(l) * 3.2);
      level.current = Math.max(v, level.current);
    });
    return () => {
      offState();
      offLevel();
    };
  }, []);

  useEffect(() => {
    if (s.state !== 'recording') return;
    let frame = 0;
    let shown = 0;
    const tick = (t: number) => {
      // Smooth: quick attack, slower release, so waves breathe with the voice.
      shown += (level.current - shown) * (level.current > shown ? 0.5 : 0.12);
      level.current *= 0.9;
      bars.current.forEach((el, i) => {
        if (!el) return;
        const wobble = 0.65 + 0.35 * Math.sin(t / 110 + i * 1.3);
        const h = 4 + shown * 22 * SHAPE[i] * wobble;
        el.style.height = `${h.toFixed(1)}px`;
      });
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [s.state]);

  return (
    <div className="capsule">
      {s.state === 'recording' && (
        <div className="wave" aria-label="Listening">
          {Array.from({ length: BARS }, (_, i) => (
            <span key={i} ref={(el) => { bars.current[i] = el; }} />
          ))}
        </div>
      )}
      {s.state === 'processing' && <span className="spinner" aria-label="Transcribing" />}
      {s.state === 'done' && <span className="check" aria-label="Pasted">✓</span>}
      {(s.state === 'empty' || s.state === 'error') && <span className="msg">{s.message}</span>}
    </div>
  );
}
