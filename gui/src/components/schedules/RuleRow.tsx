import {
  ruleName,
  ruleSummaryParts,
  scopeWarnings,
  type Rule,
  type ScopeChoices,
} from "../../lib/schedule";
import { AlertTriangle } from "../icons/AlertTriangle";
import { MoreMenu } from "./MoreMenu";
import { Sentence } from "./Sentence";
import { Switch } from "./Switch";

type Props = {
  rule: Rule;
  /** What exists now; names in the rule's scope that nothing matches get a warning. */
  choices?: ScopeChoices;
  onToggle: (on: boolean) => void;
  onEdit: () => void;
  /** Opens what the rule did; absent for a rule that keeps no history (speed limits). */
  onHistory?: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
};

// One rule: its name and sentence, a switch, and a ··· menu. A switched-off rule fades its
// text, not its switch, and keeps its settings.
export function RuleRow({
  rule,
  choices,
  onToggle,
  onEdit,
  onHistory,
  onDuplicate,
  onDelete,
}: Props) {
  const name = ruleName(rule);
  const warnings = choices ? scopeWarnings(rule.scope, choices) : [];

  return (
    <li
      data-testid="rule-row"
      data-enabled={rule.enabled}
      className="grid grid-cols-[1fr_auto_auto] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className={`min-w-0 ${rule.enabled ? "" : "text-g400"}`}>
        <div className="flex items-center gap-1.5">
          <span data-testid="rule-name" className="truncate font-medium">
            {name}
          </span>
          {warnings.length > 0 && (
            <span data-testid="rule-warning-icon" className="shrink-0 text-warn-ink">
              <AlertTriangle size={14} />
            </span>
          )}
        </div>
        <div data-testid="rule-summary" className={`text-xs ${rule.enabled ? "text-g600" : ""}`}>
          <Sentence parts={ruleSummaryParts(rule)} />
        </div>
        {warnings.map((w) => (
          <div key={w.text} data-testid="rule-warning" className="text-xs text-warn-ink">
            {w.text}
          </div>
        ))}
      </div>
      <Switch
        checked={rule.enabled}
        label={`${name} on`}
        testId="rule-switch"
        onChange={onToggle}
      />
      <MoreMenu
        label={`${name} options`}
        buttonTestId="rule-menu-button"
        menuTestId="rule-menu"
        items={[
          { label: "Edit", onSelect: onEdit },
          ...(onHistory ? [{ label: "History", onSelect: onHistory }] : []),
          { label: "Duplicate", onSelect: onDuplicate },
          { label: "Delete", onSelect: onDelete },
        ]}
      />
    </li>
  );
}
