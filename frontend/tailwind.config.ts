import type { Config } from "tailwindcss";

const config: Config = {
  content: ["./app/**/*.{ts,tsx}", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        display: ["var(--font-display)", "sans-serif"],
        body: ["var(--font-body)", "sans-serif"],
      },
      colors: {
        surface: "#020617",
        accent: {
          400: "#34d399",
          500: "#10b981",
        },
      },
    },
  },
  plugins: [],
};

export default config;
