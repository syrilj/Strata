/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'system-ui', '-apple-system', 'sans-serif'],
        mono: ['JetBrains Mono', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },
      colors: {
        zinc: {
          850: '#1c1c1f',
          950: '#09090b',
        },
        // Restrained primary: warm paper white for actions on dark.
        // Semantic accent: emerald only. Everything else is zinc.
        ink: {
          950: '#0a0a0b',
          900: '#131316',
          850: '#1a1a1e',
          800: '#232328',
        },
      },
      boxShadow: {
        // No glow, no harsh shadows — 1px hairline + single soft lift
        card: '0 1px 2px rgba(0,0,0,0.5)',
        pop: '0 8px 24px -12px rgba(0,0,0,0.7), 0 2px 6px rgba(0,0,0,0.4)',
      },
      borderRadius: {
        xl: '10px',
        lg: '8px',
      },
      keyframes: {
        'pulse-dot': {
          '0%, 100%': { opacity: '1', transform: 'scale(1)' },
          '50%': { opacity: '0.55', transform: 'scale(0.85)' },
        },
      },
      animation: {
        'pulse-dot': 'pulse-dot 2s ease-in-out infinite',
      },
      transitionTimingFunction: {
        micro: 'cubic-bezier(0.16, 1, 0.3, 1)',
      },
    },
  },
  plugins: [],
}
