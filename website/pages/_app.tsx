import { createGlobalStyle } from "styled-components";
import "xterm/css/xterm.css";
import "highlight.js/styles/obsidian.css";

const Reset = createGlobalStyle`
  html, body {
    padding: 0;
    margin: 0;
    background: #332f2d;
    color: #eee;
  }
`;

const Typography = createGlobalStyle`
  html, body, button {
    font-family: 'Courier Prime', monospace;
  }
`;

export default function MyApp({ Component, pageProps }) {
  return (
    <>
      <Reset />
      <Typography />
      <Component {...pageProps} />
    </>
  );
}
