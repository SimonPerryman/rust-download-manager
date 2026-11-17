type Props = {
  checked: boolean;
  label: string;
  onChange: (on: boolean) => void;
  testId?: string;
};

// An on/off switch. The track keeps its colour whatever the row around it does.
export function Switch({ checked, label, onChange, testId }: Props) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      data-testid={testId}
      onClick={() => onChange(!checked)}
      className={`relative h-4 w-7 shrink-0 rounded-full transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
        checked ? "bg-btn" : "bg-g400"
      }`}
    >
      <span
        className={`absolute top-0.5 size-3 rounded-full bg-white transition-all ${
          checked ? "left-3.5" : "left-0.5"
        }`}
      />
    </button>
  );
}
