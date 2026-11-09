import { useRef } from "react";
import { Search } from "../icons/Search";
import { X } from "../icons/X";

// Filters the table by filename or host as you type; Escape or × clears it.
export function NameFilter({ query, onQuery }: { query: string; onQuery: (q: string) => void }) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <div className="relative flex items-center">
      <Search className="pointer-events-none absolute left-2 text-g400" />
      <input
        ref={input}
        type="text"
        aria-label="Filter by name"
        placeholder="Filter by name"
        value={query}
        onChange={(e) => onQuery(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && onQuery("")}
        className="name-filter h-8 rounded-md border border-transparent bg-hover pl-8 pr-8 text-sm placeholder:text-g400 hover:border-line-strong focus:border-accent focus:bg-raised focus:outline-hidden"
      />
      {query && (
        <button
          aria-label="Clear filter"
          title="Clear filter"
          onClick={() => {
            onQuery("");
            input.current?.focus();
          }}
          className="icon-btn icon-btn-sm absolute right-1"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
