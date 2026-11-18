export const FIELD =
  "flex h-8 w-full rounded-md border border-line-strong bg-raised px-3 text-sm transition-colors placeholder:text-g400 focus-visible:border-accent focus-visible:outline-hidden";

// A labelled form row of the schedule forms.
export function Field({
  label,
  htmlFor,
  children,
}: {
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <label htmlFor={htmlFor} className="block text-sm font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}
