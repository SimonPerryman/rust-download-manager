import type { EmptyCopy } from "../../lib/copy";
import { Plus } from "../icons/Plus";
import { Search } from "../icons/Search";

// What an empty view says, with the next step when there is one.
export function EmptyState({ copy, onAdd }: { copy: EmptyCopy; onAdd: () => void }) {
  return (
    <div
      data-testid="empty-state"
      className="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center text-g600"
    >
      <span className="mb-2 grid h-12 w-12 place-items-center rounded-full bg-primary-soft text-primary-ink">
        {copy.invitesAdd ? <Plus size={24} /> : <Search size={24} />}
      </span>
      <h3 className="text-lg font-semibold tracking-tight text-ink">{copy.title}</h3>
      <p className="max-w-[360px]">{copy.body}</p>
      {copy.invitesAdd && (
        <button className="btn btn-soft mt-2" onClick={onAdd}>
          <Plus />
          Add download
        </button>
      )}
    </div>
  );
}
