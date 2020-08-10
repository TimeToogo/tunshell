import "xterm/css/xterm.css";
import { createGlobalStyle } from "styled-components";

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
