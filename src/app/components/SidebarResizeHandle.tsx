import { useRef, type KeyboardEvent, type PointerEvent } from 'react';

interface Props {
  side: 'left' | 'right';
  value: number;
  min: number;
  max: number;
  defaultValue: number;
  onChange: (value: number) => void;
}

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** Accessible desktop splitter. Drag, use arrow keys, or double-click to reset. */
export default function SidebarResizeHandle({ side, value, min, max, defaultValue, onChange }: Props) {
  const drag = useRef<{ x: number; value: number }>();

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { x: event.clientX, value };
    document.body.classList.add('is-resizing-sidebar');
  };
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const delta = event.clientX - drag.current.x;
    onChange(clamp(drag.current.value + (side === 'left' ? delta : -delta), min, max));
  };
  const pointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    drag.current = undefined;
    event.currentTarget.releasePointerCapture(event.pointerId);
    document.body.classList.remove('is-resizing-sidebar');
  };
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const direction = event.key === 'ArrowRight' ? 1 : -1;
    onChange(clamp(value + direction * (side === 'left' ? 8 : -8), min, max));
  };

  return (
    <div
      className={`sidebar-resize-handle resize-${side}`}
      role="separator"
      aria-label={side === 'left' ? '调整连接管理栏宽度' : '调整 AI 对话栏宽度'}
      aria-orientation="vertical"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={Math.round(value)}
      tabIndex={0}
      onPointerDown={pointerDown}
      onPointerMove={pointerMove}
      onPointerUp={pointerUp}
      onPointerCancel={pointerUp}
      onKeyDown={keyDown}
      onDoubleClick={() => onChange(defaultValue)}
      title="拖拽调整宽度，双击恢复默认"
    ><i /></div>
  );
}
