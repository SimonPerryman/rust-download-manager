import { useEffect, useId, useRef, useState } from "react";
import { Plus } from "../icons/Plus";
import { X } from "../icons/X";

/** A name in a scope row: `key` identifies it (a batch's id), `label` is what is shown. */
export interface Item {
  key: string;
  label: string;
}

type Props = {
  /** The row's name: "Tags". */
  title: string;
  /** What one name is called, for the buttons: "tag". */
  noun: string;
  chosen: Item[];
  /** What exists now, suggested as you type. */
  options: Item[];
  /** Whether a name that isn't an option can be added (tags and hosts, not batches). */
  allowNew: boolean;
  onChange: (next: Item[]) => void;
  /** Notes under the row for names nothing matches; they never block saving. */
  warnings: string[];
  /** Enter on an empty box saves the form. */
  onSubmit: () => void;
};

const MAX_SUGGESTIONS = 8;
const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

// One kind of name in a rule's scope: chips and a typing combobox that suggests what exists. An
// empty row collapses to "+ Add".
export function ScopeField({
  title,
  noun,
  chosen,
  options,
  allowNew,
  onChange,
  warnings,
  onSubmit,
}: Props) {
  const [text, setText] = useState("");
  const [expanded, setExpanded] = useState(false);
  const [listOpen, setListOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();
  const slug = noun;

  useEffect(() => {
    if (expanded) input.current?.focus();
  }, [expanded]);

  const typed = text.trim();
  const free = options.filter((o) => !chosen.some((c) => c.key === o.key));
  const matches = free
    .filter((o) => !typed || o.label.toLowerCase().includes(typed.toLowerCase()))
    .slice(0, MAX_SUGGESTIONS);
  // The typed name as a last suggestion when it is new.
  const isNew =
    allowNew &&
    typed !== "" &&
    !options.some((o) => same(o.label, typed)) &&
    !chosen.some((c) => same(c.label, typed));
  const rows: Item[] = [...matches, ...(isNew ? [{ key: typed, label: typed }] : [])];

  const add = (item: Item) => {
    if (!chosen.some((c) => c.key === item.key || same(c.label, item.label)))
      onChange([...chosen, item]);
    setText("");
    setActive(-1);
  };
  const remove = (key: string) => onChange(chosen.filter((c) => c.key !== key));

  // Adds what is typed: the highlighted suggestion, an exact option, or (where allowed) a new name.
  const commit = (): boolean => {
    if (active >= 0 && rows[active]) {
      add(rows[active]);
      return true;
    }
    if (!typed) return false;
    const exact = options.find((o) => same(o.label, typed));
    if (exact) add(exact);
    else if (allowNew) add({ key: typed, label: typed });
    else return false;
    return true;
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setListOpen(true);
      const n = rows.length;
      if (n) setActive((a) => (e.key === "ArrowDown" ? (a + 1) % n : (a - 1 + n) % n));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (!commit() && !typed) onSubmit();
    } else if (e.key === "," && allowNew) {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape" && listOpen) {
      e.stopPropagation();
      setListOpen(false);
      setActive(-1);
    } else if (e.key === "Backspace" && !text && chosen.length) {
      remove(chosen[chosen.length - 1].key);
    }
  };

  const collapsed = chosen.length === 0 && !expanded;

  return (
    <div data-testid={`scope-${slug}`} className="grid grid-cols-[4.5rem_1fr] items-start gap-x-2">
      <span className="pt-1 text-sm text-g600">{title}</span>
      <div className="min-w-0">
        {collapsed ? (
          <button
            type="button"
            data-testid={`scope-add-${slug}`}
            aria-label={`Add ${noun}`}
            onClick={() => setExpanded(true)}
            className="inline-flex h-7 items-center gap-1 rounded-md px-1 text-sm text-g600 hover:text-ink"
          >
            <Plus size={14} />
            Add
          </button>
        ) : (
          <div className="relative">
            <div
              onClick={() => input.current?.focus()}
              className="flex min-h-8 w-full flex-wrap items-center gap-1.5 rounded-md border border-line-strong bg-raised px-2 py-1 text-sm transition-colors focus-within:border-accent"
            >
              {chosen.map((c) => (
                <span key={c.key} data-testid="scope-chip" data-name={c.label} className="tag-chip">
                  <span className="truncate">{c.label}</span>
                  <button
                    type="button"
                    aria-label={`Remove ${c.label}`}
                    onClick={(e) => {
                      e.stopPropagation();
                      remove(c.key);
                      input.current?.focus();
                    }}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
              <input
                ref={input}
                type="text"
                role="combobox"
                aria-label={title}
                aria-expanded={listOpen && rows.length > 0}
                aria-controls={listId}
                aria-autocomplete="list"
                aria-activedescendant={active >= 0 ? `${listId}-${active}` : undefined}
                value={text}
                onChange={(e) => {
                  setText(e.target.value);
                  setListOpen(true);
                  setActive(-1);
                }}
                onFocus={() => setListOpen(true)}
                onBlur={() => {
                  commit();
                  setListOpen(false);
                  setActive(-1);
                  setText("");
                  if (chosen.length === 0) setExpanded(false);
                }}
                onKeyDown={onKeyDown}
                placeholder={chosen.length === 0 ? `Add a ${noun}` : ""}
                className="min-w-24 flex-1 bg-transparent py-0.5 outline-hidden placeholder:text-g400"
              />
            </div>
            {listOpen && rows.length > 0 && (
              <ul
                id={listId}
                role="listbox"
                aria-label={`${title} suggestions`}
                // Keeps the box focused while a suggestion is clicked.
                onMouseDown={(e) => e.preventDefault()}
                className="absolute inset-x-0 top-full z-10 mt-1 max-h-48 overflow-y-auto rounded-md bg-raised py-1 text-sm shadow-dialog ring-1 ring-line"
              >
                {rows.map((r, i) => (
                  <li
                    key={r.key}
                    id={`${listId}-${i}`}
                    role="option"
                    aria-selected={i === active}
                    data-testid="scope-option"
                    onClick={() => add(r)}
                    className={`cursor-pointer truncate px-3 py-1 ${i === active ? "bg-hover" : "hover:bg-hover"}`}
                  >
                    {isNew && i === rows.length - 1 ? `Add "${r.label}"` : r.label}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        {warnings.map((w) => (
          <p
            key={w}
            data-testid="scope-warning"
            className="mt-1 flex items-start gap-1 text-xs text-warn-ink"
          >
            {w}
          </p>
        ))}
      </div>
    </div>
  );
}
