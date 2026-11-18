import { ruleName, ruleSummary, type Rule } from "../../lib/schedule";
import { MoreMenu } from "./MoreMenu";
import { Switch } from "./Switch";

type Props = {
  rule: Rule;
  onToggle: (on: boolean) => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
};

// One rule: its name and sentence, a switch, and a ··· menu. A switched-off rule fades its
// text, not its switch, and keeps its settings.
export function RuleRow({ rule, onToggle, onEdit, onDuplicate, onDelete }: Props) {
  const name = ruleName(rule);

  return (
    <li
      data-testid="rule-row"
      data-enabled={rule.enabled}
      className="grid grid-cols-[1fr_auto_auto] items-center gap-3 border-t border-line px-3 py-2 first:border-t-0"
    >
      <div className={`min-w-0 ${rule.enabled ? "" : "text-g400"}`}>
        <div data-testid="rule-name" className="truncate font-medium">
          {name}
        </div>
        <div data-testid="rule-summary" className={`text-xs ${rule.enabled ? "text-g600" : ""}`}>
          {ruleSummary(rule)}
        </div>
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
          { label: "Duplicate", onSelect: onDuplicate },
          { label: "Delete", onSelect: onDelete },
        ]}
      />
    </li>
  );
}
