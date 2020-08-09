import "xterm/css/xterm.css";
import { createGlobalStyle } from "styled-components";

const Reset = createGlobalStyle`
  html, body {
    padding: 0;
    margin: 0;
    background: #333;
    color: #eee;
  }
`;

const Typography = createGlobalStyle`
  html, body {
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
