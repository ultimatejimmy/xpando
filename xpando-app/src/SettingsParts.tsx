import { useId, type ReactNode } from "react";
import "./settings.css";

/** A titled group of rows in the Settings dialog. */
export function SettingsGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="st-group">
      <h3 className="st-heading">{title}</h3>
      <div className="st-card">{children}</div>
    </section>
  );
}

/**
 * One setting: its name and a short explanation on the left, its control on the right.
 * `below` goes under the text, across the row's width (a folder path, for example).
 */
export function SettingsRow({
  title,
  help,
  below,
  children,
  id,
}: {
  title: string;
  help?: ReactNode;
  below?: ReactNode;
  children?: ReactNode;
  id?: string;
}) {
  return (
    <div className="st-row">
      <div className="st-text">
        <p className="st-title" id={id}>
          {title}
        </p>
        {help && <p className="st-help">{help}</p>}
        {below}
      </div>
      {children && <div className="st-control">{children}</div>}
    </div>
  );
}

/** A row whose control is an on/off switch. */
export function SwitchRow({
  title,
  help,
  checked,
  disabled,
  onChange,
}: {
  title: string;
  help?: ReactNode;
  checked: boolean;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  const id = useId();
  return (
    <SettingsRow title={title} help={help} id={id}>
      <input
        type="checkbox"
        role="switch"
        className="st-switch"
        aria-labelledby={id}
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </SettingsRow>
  );
}
