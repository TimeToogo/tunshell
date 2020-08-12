import { createGlobalStyle } from "styled-components";
import "xterm/css/xterm.css";
import "highlight.js/styles/obsidian.css";
import { COLOURS } from "../theme/colours";

const Reset = createGlobalStyle`
  html, body {
    padding: 0;
    margin: 0;
    background: ${COLOURS.TAN3};
    color: ${COLOURS.OFF_WHITE};
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
