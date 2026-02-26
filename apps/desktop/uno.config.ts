import {
  defineConfig,
  presetAttributify,
  presetUno,
  transformerVariantGroup,
} from "unocss";

export default defineConfig({
  presets: [presetUno(), presetAttributify()],
  transformers: [transformerVariantGroup()],
  theme: {
    breakpoints: {
      lg: "1050px",
    },
    colors: {
      bg: "#f2efe8",
      bg2: "#ebe5d8",
      text: "#17202a",
      muted: "#556170",
      brand: "#0a5f54",
      accent: "#ec8a2f",
      danger: "#b43f2a",
      line: "#17202a1f",
      card: "#ffffffd1",
      sidebar: "#ffffff61",
      control: "#ffffffe6",
      code: "#fdfcf9",
    },
    fontFamily: {
      sans: "\"Avenir Next\", \"PingFang SC\", \"Microsoft YaHei\", sans-serif",
    },
  },
  preflights: [
    {
      getCSS: () => `
        :root {
          color-scheme: light;
          --color-bg: #f2efe8;
          --color-bg-2: #ebe5d8;
          --color-text: #17202a;
          --color-muted: #556170;
          --color-brand: #0a5f54;
          --color-accent: #ec8a2f;
          --color-danger: #b43f2a;
          --color-line: #17202a1f;
          --color-card: #ffffffd1;
          --radius-panel: 16px;
          --radius-control: 10px;
          --space-page: 20px;
          --space-control-x: 12px;
          --space-control-y: 10px;
          --font-size-body: 15px;
          --font-size-control: 16px;
          --line-height-control: 1.45;
          --shadow-control-focus: 0 0 0 3px rgb(10 95 84 / 12%);
        }

        * {
          box-sizing: border-box;
        }

        body {
          margin: 0;
          color: var(--color-text);
          font-family: "Avenir Next", "PingFang SC", "Microsoft YaHei", sans-serif;
          font-size: var(--font-size-body);
          background:
            radial-gradient(circle at 15% 20%, rgb(236 138 47 / 20%), transparent 30%),
            radial-gradient(circle at 80% 10%, rgb(10 95 84 / 25%), transparent 40%),
            linear-gradient(160deg, var(--color-bg) 0%, var(--color-bg-2) 100%);
        }

        #app {
          min-height: 100vh;
        }

        h2, h3, h4 {
          margin: 0;
        }

        input:not([type="checkbox"]):not([type="radio"]),
        select,
        textarea {
          width: 100%;
          border: 1px solid var(--color-line);
          border-radius: var(--radius-control);
          padding: var(--space-control-y) var(--space-control-x);
          background: rgb(255 255 255 / 90%);
          color: var(--color-text);
          font-size: var(--font-size-control);
          line-height: var(--line-height-control);
          transition: border-color 0.2s ease, box-shadow 0.2s ease, background-color 0.2s ease;
        }

        input:not([type="checkbox"]):not([type="radio"]):focus,
        select:focus,
        textarea:focus {
          outline: none;
          border-color: rgb(10 95 84 / 45%);
          box-shadow: var(--shadow-control-focus);
          background: #fff;
        }

        input[type="checkbox"],
        input[type="radio"] {
          width: 16px;
          height: 16px;
          margin: 0;
          padding: 0;
          accent-color: var(--color-brand);
        }

        textarea {
          resize: vertical;
          min-height: 88px;
        }

        a {
          color: var(--color-brand);
          text-decoration: none;
        }

        a:hover {
          text-decoration: underline;
        }
      `,
    },
  ],
});
