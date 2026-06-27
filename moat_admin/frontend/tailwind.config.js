/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: ["class"],
  content: [
    "./src/pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/components/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/app/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/**/*.{js,ts,jsx,tsx,mdx}"
  ],
  theme: {
    extend: {
      colors: {
        background: "#000000",
        foreground: "#ffffff",
        primary: {
          DEFAULT: "#ffffff",
          foreground: "#000000",
          container: "#1f1f1f",
          onContainer: "#ffffff"
        },
        secondary: {
          DEFAULT: "#a3a3a3",
          foreground: "#171717",
          container: "#262626"
        },
        surface: {
          DEFAULT: "#000000",
          dim: "#050505",
          bright: "#171717",
          container: {
            DEFAULT: "#0a0a0a",
            low: "#030303",
            lowest: "#000000",
            high: "#121212",
            highest: "#1c1c1c"
          }
        },
        on: {
          surface: "#ffffff",
          surfaceVariant: "#a3a3a3"
        },
        outline: {
          DEFAULT: "#525252",
          variant: "#262626"
        },
        error: {
          DEFAULT: "#ff716c",
          container: "#8a1a1e",
          dim: "#c94947",
          foreground: "#490006"
        },
        muted: {
          DEFAULT: "#0a0a0a",
          foreground: "#737373"
        },
        accent: {
          DEFAULT: "#171717",
          foreground: "#ffffff"
        },
        card: {
          DEFAULT: "#0a0a0a",
          foreground: "#ffffff"
        },
        border: "#262626"
      },
      borderRadius: {
        lg: "0.5rem",
        md: "0.375rem",
        sm: "0.25rem"
      },
      fontFamily: {
        sans: ["Inter", "sans-serif"],
        mono: ["Geist Mono", "monospace"]
      }
    }
  },
  plugins: []
}
