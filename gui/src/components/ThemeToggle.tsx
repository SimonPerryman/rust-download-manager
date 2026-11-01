import { useEffect, useState } from "react";
import { Sun, Moon } from "./icons";

// Toggles the `.dark` class on <html>, exactly as the mockup's script did.
export function ThemeToggle() {
  const [dark, setDark] = useState(false);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
  }, [dark]);

  return (
    <button
      onClick={() => setDark((d) => !d)}
      aria-label="Toggle theme"
      className="inline-flex h-9 w-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      {dark ? <Moon size={17} /> : <Sun size={17} />}
    </button>
  );
}
