import { useCallback, useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChartColumnBig, X } from "lucide-react";
import { Dialog, useDialog } from "./FolderManager";
import { SettingsGroup, SettingsRow, SwitchRow } from "./SettingsParts";
import "./stats.css";

type Range = "today" | "week" | "month" | "all";

interface StatsSettings {
  pay_type: "hourly" | "salaried";
  hourly_rate: number;
  annual_salary: number;
  hours_per_week: number;
  speed_mode: "auto" | "manual";
  manual_wpm: number;
}

interface Summary {
  enabled: boolean;
  expansions: number;
  chars: number;
  seconds_saved: number;
  previous_seconds_saved: number | null;
  money_saved: number | null;
  hourly_rate: number | null;
  wpm: number;
  wpm_measured: boolean;
  best_day: number;
  days: { date: string; count: number }[];
  top: { trigger: string; count: number; seconds_saved: number }[];
  settings: StatsSettings;
}

const RANGES: { id: Range; label: string; previous: string | null }[] = [
  { id: "today", label: "Today", previous: null },
  { id: "week", label: "7 days", previous: "the 7 days before" },
  { id: "month", label: "30 days", previous: "the 30 days before" },
  { id: "all", label: "All time", previous: null },
];

const WORKDAY_SECONDS = 8 * 3600;

/** "7 h 04 min", "12 min", "40 sec". */
export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s} sec`;
  const totalMin = Math.round(s / 60);
  if (totalMin < 60) return `${totalMin} min`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return `${h} h ${String(m).padStart(2, "0")} min`;
}

const money = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", maximumFractionDigits: 0 });
const count = new Intl.NumberFormat();

function dayLabel(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
}

export function StatsButton({ onClick }: { onClick: () => void }) {
  return (
    <button type="button" className="btn btn-icon" aria-label="Your savings" title="Your savings" onClick={onClick}>
      <ChartColumnBig size={18} aria-hidden="true" />
    </button>
  );
}

/**
 * Your savings: how much typing the phrases replaced, as time and money.
 * `names` maps a trigger to the phrase's name, for the top list.
 */
export function StatsDialog({ onClose, names }: { onClose: () => void; names: Record<string, string> }) {
  const dialog = useDialog(onClose);
  const titleId = useId();
  const [range, setRange] = useState<Range>("month");
  const [data, setData] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [testing, setTesting] = useState(false);

  const load = useCallback(
    (r: Range) =>
      invoke<Summary>("stats_summary", { range: r, tzOffsetMin: -new Date().getTimezoneOffset() })
        .then((s) => {
          setData(s);
          setError(null);
        })
        .catch((e) => setError(String(e))),
    [],
  );

  useEffect(() => {
    void load(range);
  }, [range, load]);

  const saveSettings = (settings: StatsSettings) => {
    setData((d) => (d ? { ...d, settings } : d));
    invoke<StatsSettings>("stats_save_settings", { settings })
      .then(() => load(range))
      .catch((e) => setError(String(e)));
  };

  const setEnabled = (on: boolean) => {
    invoke("stats_set_enabled", { on })
      .then(() => load(range))
      .catch((e) => setError(String(e)));
  };

  const reset = () => {
    setConfirmReset(false);
    invoke("stats_reset")
      .then(() => load(range))
      .catch((e) => setError(String(e)));
  };

  const rangeInfo = RANGES.find((r) => r.id === range)!;
  const prev = data?.previous_seconds_saved ?? 0;
  const change =
    data && rangeInfo.previous && prev > 0 ? Math.round(((data.seconds_saved - prev) / prev) * 100) : null;
  const maxDay = Math.max(1, ...(data?.days.map((d) => d.count) ?? [1]));
  const maxTop = Math.max(1, ...(data?.top.map((t) => t.seconds_saved) ?? [1]));

  return (
    <Dialog title="Your savings" labelId={titleId} dialog={dialog}>
      <div className="stats">
        <div className="stats-bar">
          <div className="stats-seg" role="radiogroup" aria-label="Period">
            {RANGES.map((r) => (
              <button
                key={r.id}
                type="button"
                role="radio"
                aria-checked={range === r.id}
                className={range === r.id ? "on" : ""}
                onClick={() => setRange(r.id)}
              >
                {r.label}
              </button>
            ))}
          </div>
        </div>

        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}

        {data && !data.enabled && (
          <p className="stats-note">
            Statistics are paused, so new expansions are not counted.{" "}
            <button type="button" className="stats-link" onClick={() => setEnabled(true)}>
              Turn on
            </button>
          </p>
        )}

        <div className="stats-grid">
        <div className="stats-main">
        <section className="stats-hero" aria-live="polite">
          <p className="stats-hero-k">xpando saved you</p>
          <p className="stats-hero-v">{data ? formatDuration(data.seconds_saved) : "..."}</p>
          {data && (
            <p className="stats-hero-s">
              {data.money_saved !== null ? (
                <>
                  about {money.format(data.money_saved)} of your time
                  {data.seconds_saved >= 0.1 * WORKDAY_SECONDS &&
                    `, roughly ${(data.seconds_saved / WORKDAY_SECONDS).toFixed(1)} workdays`}
                </>
              ) : (
                <>Add your pay below to see what that is worth.</>
              )}
              {change !== null && (
                <>
                  {" "}
                  · {change >= 0 ? "+" : ""}
                  {change}% vs {rangeInfo.previous}
                </>
              )}
            </p>
          )}
        </section>

        <div className="stats-tiles">
          <div className="stats-tile">
            <p className="k">Expansions</p>
            <p className="v">{count.format(data?.expansions ?? 0)}</p>
          </div>
          <div className="stats-tile">
            <p className="k">Characters</p>
            <p className="v">{count.format(data?.chars ?? 0)}</p>
          </div>
          <div className="stats-tile">
            <p className="k">Typing speed</p>
            <p className="v">{Math.round(data?.wpm ?? 0)} WPM</p>
            <p className="s">
              {data?.settings.speed_mode === "manual"
                ? "set by you"
                : data?.wpm_measured
                  ? "measured"
                  : "estimated, still learning"}
            </p>
          </div>
          <div className="stats-tile">
            <p className="k">Best day</p>
            <p className="v">{count.format(data?.best_day ?? 0)}</p>
          </div>
        </div>

        {testing && data ? (
          <section>
            <h3 className="stats-h">Typing test</h3>
            <TypingTest
              onUse={(wpm) => {
                saveSettings({ ...data.settings, speed_mode: "manual", manual_wpm: wpm });
                setTesting(false);
              }}
              onClose={() => setTesting(false)}
            />
          </section>
        ) : (
          <>
        <section>
          <h3 className="stats-h">Expansions per day</h3>
          <div
            className="stats-bars"
            role="img"
            aria-label={`Expansions per day over the last ${data?.days.length ?? 14} days. ${
              data?.days.map((d) => `${dayLabel(d.date)}: ${d.count}`).join(", ") ?? ""
            }`}
          >
            {data?.days.map((d) => (
              <div key={d.date} className="stats-col" title={`${dayLabel(d.date)}: ${d.count}`}>
                <i style={{ height: `${Math.max(d.count > 0 ? 4 : 0, (d.count / maxDay) * 100)}%` }} />
              </div>
            ))}
          </div>
        </section>

        <section>
          <h3 className="stats-h">Top phrases by time saved</h3>
          {data && data.top.length === 0 ? (
            <p className="stats-empty">
              No expansions yet in this period. Type a trigger and your savings show up here.
            </p>
          ) : (
            <ol className="stats-top">
              {data?.top.slice(0, 5).map((t, i) => (
                <li key={t.trigger}>
                  <span className="n">{i + 1}</span>
                  <span className="name">
                    <code>{t.trigger}</code>
                    {names[t.trigger] && <span className="label">{names[t.trigger]}</span>}
                  </span>
                  <span className="meter" aria-hidden="true">
                    <i style={{ width: `${Math.max(3, (t.seconds_saved / maxTop) * 100)}%` }} />
                  </span>
                  <span className="time">
                    {formatDuration(t.seconds_saved)}
                    <span className="uses"> · {count.format(t.count)}×</span>
                  </span>
                </li>
              ))}
            </ol>
          )}
        </section>

          </>
        )}
        </div>
        <aside className="stats-side">
        {data && (
          <SavingsSettings
            data={data}
            onSave={saveSettings}
            onEnabled={setEnabled}
            testing={testing}
            onToggleTest={() => setTesting((t) => !t)}
            confirmReset={confirmReset}
            onAskReset={() => setConfirmReset(true)}
            onCancelReset={() => setConfirmReset(false)}
            onReset={reset}
          />
        )}
        </aside>
        </div>

        <div className="st-footer">
          <button type="button" className="btn btn-primary" onClick={dialog.close}>
            Done
          </button>
        </div>
      </div>
    </Dialog>
  );
}

function num(text: string): number {
  const n = Number(text.replace(/[^0-9.]/g, ""));
  return Number.isFinite(n) ? n : 0;
}

function NumberField({
  label,
  value,
  onCommit,
  prefix,
  suffix,
  width = 84,
}: {
  label: string;
  value: number;
  onCommit: (n: number) => void;
  prefix?: string;
  suffix?: string;
  width?: number;
}) {
  const [text, setText] = useState(value ? String(value) : "");
  useEffect(() => setText(value ? String(value) : ""), [value]);
  const commit = () => {
    const n = num(text);
    if (n !== value) onCommit(n);
  };
  return (
    <label className="stats-num">
      {prefix}
      <input
        className="field"
        inputMode="decimal"
        aria-label={label}
        style={{ width }}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
      />
      {suffix}
    </label>
  );
}

function SavingsSettings({
  data,
  onSave,
  onEnabled,
  testing,
  onToggleTest,
  confirmReset,
  onAskReset,
  onCancelReset,
  onReset,
}: {
  data: Summary;
  onSave: (s: StatsSettings) => void;
  onEnabled: (on: boolean) => void;
  testing: boolean;
  onToggleTest: () => void;
  confirmReset: boolean;
  onAskReset: () => void;
  onCancelReset: () => void;
  onReset: () => void;
}) {
  const s = data.settings;
  const set = (patch: Partial<StatsSettings>) => onSave({ ...s, ...patch });
  return (
    <SettingsGroup title="Savings settings">
      <SettingsRow title="Pay type" help="Used to turn the time you save into money.">
        <div className="stats-seg" role="radiogroup" aria-label="Pay type">
          {(["hourly", "salaried"] as const).map((p) => (
            <button
              key={p}
              type="button"
              role="radio"
              aria-checked={s.pay_type === p}
              className={s.pay_type === p ? "on" : ""}
              onClick={() => set({ pay_type: p })}
            >
              {p === "hourly" ? "Hourly" : "Salaried"}
            </button>
          ))}
        </div>
      </SettingsRow>
      {s.pay_type === "hourly" ? (
        <SettingsRow title="Hourly rate">
          <NumberField label="Hourly rate" prefix="$" suffix="per hour" value={s.hourly_rate} onCommit={(n) => set({ hourly_rate: n })} />
        </SettingsRow>
      ) : (
        <SettingsRow
          title="Yearly pay"
          help={
            data.hourly_rate !== null
              ? `Works out to ${money.format(data.hourly_rate)} per hour (${s.hours_per_week} hours a week, 52 weeks).`
              : "Enter your yearly pay and the hours you work each week."
          }
          below={
            <div className="stats-below">
              <NumberField label="Yearly pay" prefix="$" suffix="a year" width={104} value={s.annual_salary} onCommit={(n) => set({ annual_salary: n })} />
              <NumberField label="Hours per week" suffix="hours a week" width={56} value={s.hours_per_week} onCommit={(n) => set({ hours_per_week: n })} />
            </div>
          }
        />
      )}
      <SettingsRow
        title="Typing speed"
        help={
          <>
            {s.speed_mode === "auto"
              ? data.wpm_measured
                ? `Measured as you type: ${Math.round(data.wpm)} WPM. Only timing and character counts are kept, never what you type.`
                : "Measured as you type, nothing to set up. Until it has seen enough, 40 WPM is assumed."
              : "The speed you entered."}{" "}
            <button type="button" className="stats-link" onClick={onToggleTest}>
              {testing ? "Close the test" : "Prefer to check it? Try a 30 s test."}
            </button>
          </>
        }
        below={
          s.speed_mode === "manual" ? (
            <div className="stats-below">
              <NumberField label="Words per minute" suffix="WPM" width={56} value={s.manual_wpm} onCommit={(n) => set({ manual_wpm: Math.max(1, n) })} />
            </div>
          ) : undefined
        }
      >
        <div className="stats-seg" role="radiogroup" aria-label="Typing speed">
          {(["auto", "manual"] as const).map((m) => (
            <button
              key={m}
              type="button"
              role="radio"
              aria-checked={s.speed_mode === m}
              className={s.speed_mode === m ? "on" : ""}
              onClick={() => set({ speed_mode: m })}
            >
              {m === "auto" ? "Auto" : "Manual"}
            </button>
          ))}
        </div>
      </SettingsRow>
      <SwitchRow
        title="Count my expansions"
        help="Turn off to stop recording. Counts stay on this computer and hold no phrase text."
        checked={data.enabled}
        onChange={onEnabled}
      />
      <SettingsRow title="Reset statistics" help="Deletes the expansion history. Your pay and typing speed stay.">
        {confirmReset ? (
          <>
            <button type="button" className="btn btn-danger stats-small" onClick={onReset}>
              Delete history
            </button>
            <button type="button" className="btn btn-quiet stats-small" onClick={onCancelReset}>
              Keep it
            </button>
          </>
        ) : (
          <button type="button" className="btn stats-small" onClick={onAskReset}>
            Reset
          </button>
        )}
      </SettingsRow>
    </SettingsGroup>
  );
}

// Arthur Conan Doyle, "A Scandal in Bohemia" (1891), public domain.
const SAMPLE =
  "To Sherlock Holmes she is always the woman. I have seldom heard him mention her under any other name. In his eyes she eclipses and predominates the whole of her sex. It was not that he felt any emotion akin to love for Irene Adler. All emotions, and that one particularly, were abhorrent to his cold, precise but admirably balanced mind.";
const SOURCE = "Arthur Conan Doyle, A Scandal in Bohemia";
const TEST_SECONDS = 30;

/** A 30 second typing test; the clock starts at the first key. */
function TypingTest({ onUse, onClose }: { onUse: (wpm: number) => void; onClose: () => void }) {
  const [typed, setTyped] = useState("");
  const [left, setLeft] = useState<number | null>(null);
  const [result, setResult] = useState<number | null>(null);
  const startedAt = useRef<number | null>(null);
  const typedRef = useRef("");
  const area = useRef<HTMLTextAreaElement>(null);

  useEffect(() => area.current?.focus(), []);

  /** Counts the characters that match the passage and turns them into words per minute. */
  const finish = (seconds: number) => {
    const t = typedRef.current;
    let right = 0;
    for (let i = 0; i < t.length && i < SAMPLE.length; i++) if (t[i] === SAMPLE[i]) right++;
    setResult(Math.max(1, Math.round(right / 5 / (Math.max(seconds, 1) / 60))));
  };

  useEffect(() => {
    if (left === null || result !== null) return;
    const id = window.setInterval(() => {
      const elapsed = (Date.now() - (startedAt.current ?? Date.now())) / 1000;
      const remaining = Math.max(0, TEST_SECONDS - elapsed);
      setLeft(Math.ceil(remaining));
      if (remaining <= 0) finish(TEST_SECONDS);
    }, 250);
    return () => window.clearInterval(id);
  }, [left === null, result]); // eslint-disable-line react-hooks/exhaustive-deps

  const onChange = (value: string) => {
    if (result !== null) return;
    if (startedAt.current === null) {
      startedAt.current = Date.now();
      setLeft(TEST_SECONDS);
    }
    const next = value.slice(0, SAMPLE.length);
    typedRef.current = next;
    setTyped(next);
    if (next.length === SAMPLE.length) finish((Date.now() - startedAt.current) / 1000);
  };

  const again = () => {
    startedAt.current = null;
    typedRef.current = "";
    setTyped("");
    setLeft(null);
    setResult(null);
    area.current?.focus();
  };

  return (
    <div className="stats-test">
      <p className="stats-test-text" aria-hidden="true">
        {[...SAMPLE].map((c, i) => (
          <span key={i} className={i >= typed.length ? "" : typed[i] === c ? "ok" : "bad"}>
            {c}
          </span>
        ))}
      </p>
      <p className="stats-test-src">{SOURCE}</p>
      <textarea
        className="field"
        ref={area}
        rows={2}
        aria-label="Type the passage above"
        placeholder="Start typing the passage above. The 30 seconds begin at your first key."
        value={typed}
        disabled={result !== null}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="stats-test-foot">
        {result === null ? (
          <span className="stats-test-time">{left === null ? "30 s" : `${left} s`}</span>
        ) : (
          <>
            <strong>{result} WPM</strong>
            <button type="button" className="btn btn-primary" onClick={() => onUse(result)}>
              Use this speed
            </button>
            <button type="button" className="btn" onClick={again}>
              Try again
            </button>
          </>
        )}
        <button type="button" className="btn btn-quiet" onClick={onClose} aria-label="Close the test">
          <X size={16} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
