/** @type {import('tailwindcss').Config} */
// The Console palette (.ai-cache/mockups/m5/d-combined.html). Every colour is a
// CSS variable from index.css; several are hex in one theme and rgba in the
// other, so there is no `<alpha-value>` and no `/opacity` modifiers on them.
const token = (name) => `var(--${name})`;

const COLOR_TOKENS = [
  "ink",
  "g600",
  "g400",
  "g300",
  "g200",
  "surface",
  "sidebar",
  "raised",
  "hover",
  "line",
  "line-strong",
  "track",
  "accent",
  "btn",
  "btn-h",
  "on-btn",
  "fill",
  "primary-soft",
  "primary-soft-h",
  "primary-ink",
  "sel",
  "success",
  "success-soft",
  "success-ink",
  "warn",
  "warn-soft",
  "warn-ink",
  "danger",
  "danger-fill",
  "danger-soft",
  "danger-ink",
  "muted-fill",
  "toast-bg",
  "toast-ink",
  "toast-action",
  "success-linger",
];

export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    // The design plan's type scale: 12 13 14 16 20 24.
    fontSize: {
      xs: ["12px", "16px"],
      sm: ["13px", "20px"],
      base: ["14px", "20px"],
      lg: ["16px", "24px"],
      xl: ["20px", "28px"],
      "2xl": ["24px", "32px"],
    },
    borderRadius: {
      none: "0",
      sm: "4px",
      md: "6px",
      lg: "8px",
      full: "9999px",
    },
    extend: {
      colors: Object.fromEntries(COLOR_TOKENS.map((name) => [name, token(name)])),
      boxShadow: {
        raised: token("sh-raised"),
        pop: token("sh-pop"),
        dialog: token("sh-dialog"),
      },
      fontFamily: {
        sans: ["IBM Plex Sans", "system-ui", "-apple-system", "Segoe UI", "sans-serif"],
        mono: ["IBM Plex Mono", "ui-monospace", "Consolas", "monospace"],
      },
    },
  },
  plugins: [],
};
