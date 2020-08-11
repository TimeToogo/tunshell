import * as Styled from "./styled";
import { useState, useRef, useEffect } from "react";
import hjs from "highlight.js";

interface Props {
  script: string;
  lang: string;
}

export const Script: React.FC<Props> = ({ script, lang }) => {
  const scriptRef = useRef();
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!scriptRef.current) {
      return;
    }

    hjs.highlightBlock(scriptRef.current);
  }, [scriptRef.current, lang, script]);

  const copyToClipboard = () => {
    navigator.clipboard
      .writeText(script)
      .then(() => setCopied(true))
      .then(() => new Promise((r) => setTimeout(r, 2000)))
      .then(() => setCopied(false));
  };

  return (
    <Styled.Wrapper>
      <span className={lang} ref={scriptRef}>
        {script}
      </span>

      <Styled.Copy onClick={copyToClipboard}>
        <ion-icon name="clipboard-outline" />
        <Styled.Copied active={copied}>Copied</Styled.Copied>
      </Styled.Copy>
    </Styled.Wrapper>
  );
};
