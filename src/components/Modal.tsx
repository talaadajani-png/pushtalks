import type { ReactNode } from 'react';

export default function Modal({ children, onClose }: { children: ReactNode; onClose: () => void }) {
  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="card modal stack">{children}</div>
    </div>
  );
}
