/** A thin handle between two panes: drag it, or use the arrow keys, to change the width of the pane before it. */
export function PaneGrip({
  label,
  width,
  min,
  max,
  fallback,
  start,
  onChange,
}: {
  label: string;
  /** Current width of the pane before the handle, in px. */
  width: number;
  min: number;
  max: number;
  /** Width a double-click goes back to. */
  fallback: number;
  /** Distance from the left edge of the pane area to where that pane starts, in px. */
  start: number;
  onChange: (width: number) => void;
}) {
  const clamp = (w: number) => Math.min(max, Math.max(min, Math.round(w)));
  return (
    <div
      className="pane-grip"
      style={{ left: start + width }}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={(e) => e.currentTarget.setPointerCapture(e.pointerId)}
      onPointerMove={(e) => {
        if (e.buttons !== 1) return;
        const left = e.currentTarget.parentElement!.getBoundingClientRect().left;
        onChange(clamp(e.clientX - left - start));
      }}
      onKeyDown={(e) => {
        if (e.key === "ArrowLeft") onChange(clamp(width - 16));
        else if (e.key === "ArrowRight") onChange(clamp(width + 16));
        else return;
        e.preventDefault();
      }}
      onDoubleClick={() => onChange(fallback)}
    />
  );
}
